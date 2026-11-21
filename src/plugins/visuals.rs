#![allow(clippy::type_complexity, clippy::too_many_arguments)]

use std::collections::HashMap;

use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;

use crate::types::*;

/// Plugin handling all procedural 3D PBR terrain, towers, enemies, projectiles,
/// lighting, shadows, status visuals, 3D health bars, lightning arcs, ground hazards,
/// and pooled particles.
pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VisualCache>()
            .add_systems(Startup, init_visual_cache)
            .add_systems(
                Update,
                spawn_biome_and_map_visuals.in_set(GameSet::PathfindingSet),
            )
            .add_systems(
                Update,
                (
                    spawn_tower_visuals,
                    spawn_enemy_visuals,
                    spawn_projectile_visuals,
                    spawn_hazard_visuals,
                    handle_vfx_messages,
                )
                    .in_set(GameSet::VfxUiSet),
            )
            .add_systems(
                Update,
                (
                    animate_towers,
                    animate_enemies_and_status,
                    animate_map_accents_and_torches,
                    update_health_bars,
                    render_lightning_arcs_and_status_rings,
                    update_particles_and_hazards,
                )
                    .in_set(GameSet::VfxUiSet)
                    .after(spawn_tower_visuals)
                    .after(spawn_enemy_visuals)
                    .after(handle_vfx_messages),
            );
    }
}

// ============================================================================
// Local Visual Components & Cached Resource
// ============================================================================

#[derive(Component)]
struct BiomeVisualProp;

#[derive(Component)]
struct MapPortalRing {
    spin_speed: f32,
    base_y: f32,
    pulse_phase: f32,
}

#[derive(Component)]
struct FlickeringTorchLight {
    base_intensity: f32,
    phase: f32,
}

#[derive(Component)]
struct EnemyAnimBody {
    enemy: Entity,
    kind: EnemyAnimKind,
    base_y: f32,
    scale: f32,
}

#[derive(Clone, Copy)]
enum EnemyAnimKind {
    Hop,
    FloatSway,
    SquashStretch,
    HeavyStomp,
    FlyBob,
}

#[derive(Component)]
struct EnemyWing {
    enemy: Entity,
    side: f32, // -1.0 left, 1.0 right
    base_y: f32,
}

#[derive(Component)]
struct EnemyFreezeShell {
    enemy: Entity,
}

#[derive(Component)]
struct EnemyEnrageTint {
    enemy: Entity,
}

#[derive(Component)]
struct EnemyLoopRing {
    enemy: Entity,
}

#[derive(Component)]
struct ProjectileVisualAdded;

#[derive(Component)]
struct HazardVisualAdded;

#[derive(Component)]
struct ReclaimOverlayTile {
    coord: TileCoord,
}

#[derive(Resource, Default)]
pub struct VisualCache {
    pub initialized: bool,
    pub built_map_version: Option<(MapId, i32, i32)>,
    pub tile_mesh: Handle<Mesh>,
    pub unit_cube: Handle<Mesh>,
    pub unit_sphere: Handle<Mesh>,
    pub unit_cylinder: Handle<Mesh>,
    pub unit_cone: Handle<Mesh>,
    pub unit_torus: Handle<Mesh>,
    pub thin_torus: Handle<Mesh>,
    pub crystal_frustum: Handle<Mesh>,
    pub flat_disc: Handle<Mesh>,
    pub capsule_mesh: Handle<Mesh>,
    pub hp_bg_mesh: Handle<Mesh>,
    pub hp_fill_mesh: Handle<Mesh>,
    pub bolt_mesh: Handle<Mesh>,
    pub wing_mesh: Handle<Mesh>,

    // Common reusable materials
    pub mat_mortar_dark: Handle<StandardMaterial>,
    pub mat_shadow_disc: Handle<StandardMaterial>,
    pub mat_ice_shell: Handle<StandardMaterial>,
    pub mat_hp_bg: Handle<StandardMaterial>,
    pub mat_hp_green: Handle<StandardMaterial>,
    pub mat_hp_yellow: Handle<StandardMaterial>,
    pub mat_hp_red: Handle<StandardMaterial>,
    pub mat_poison_pool: Handle<StandardMaterial>,
    pub mat_enrage_aura: Handle<StandardMaterial>,
    pub mat_loop_ring: Handle<StandardMaterial>,
    pub mat_reclaim_lock: Handle<StandardMaterial>,
    pub mat_gold: Handle<StandardMaterial>,
    pub mat_dark_iron: Handle<StandardMaterial>,
    pub mat_wood: Handle<StandardMaterial>,
    pub mat_bone: Handle<StandardMaterial>,
    pub mat_obsidian: Handle<StandardMaterial>,
    pub mat_marble: Handle<StandardMaterial>,
    pub mat_stone: Handle<StandardMaterial>,
    pub mat_glacier_stone: Handle<StandardMaterial>,
    pub mat_tree_trunk: Handle<StandardMaterial>,
    pub mat_tree_pine_grass: Handle<StandardMaterial>,
    pub mat_tree_pine_ice: Handle<StandardMaterial>,
    pub mat_torch_flame: Handle<StandardMaterial>,
    pub mat_palette: HashMap<u64, Handle<StandardMaterial>>,
}

impl VisualCache {
    fn color_mat(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        color: Color,
        roughness: f32,
        metallic: f32,
        emissive: Option<LinearRgba>,
    ) -> Handle<StandardMaterial> {
        let s = color.to_srgba();
        let e = emissive.unwrap_or(LinearRgba::BLACK);
        let key = ((s.red * 255.0) as u64)
            | (((s.green * 255.0) as u64) << 8)
            | (((s.blue * 255.0) as u64) << 16)
            | (((s.alpha * 255.0) as u64) << 24)
            | (((roughness * 100.0) as u64) << 32)
            | (((metallic * 100.0) as u64) << 40)
            | ((((e.red + e.green + e.blue).clamp(0.0, 20.0) * 10.0) as u64) << 48);

        if let Some(handle) = self.mat_palette.get(&key) {
            return handle.clone();
        }
        let handle = materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: roughness,
            metallic,
            emissive: e,
            ..default()
        });
        self.mat_palette.insert(key, handle.clone());
        handle
    }
}

fn init_visual_cache(
    mut cache: ResMut<VisualCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if cache.initialized {
        return;
    }
    cache.initialized = true;

    cache.tile_mesh = meshes.add(Cuboid::new(0.96, 0.26, 0.96));
    cache.unit_cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    cache.unit_sphere = meshes.add(Sphere::new(0.5));
    cache.unit_cylinder = meshes.add(Cylinder::new(0.5, 1.0));
    cache.unit_cone = meshes.add(Cone::new(0.5, 1.0));
    cache.unit_torus = meshes.add(Torus::new(0.38, 0.50));
    cache.thin_torus = meshes.add(Torus::new(0.44, 0.50));
    cache.crystal_frustum = meshes.add(ConicalFrustum {
        radius_top: 0.16,
        radius_bottom: 0.44,
        height: 1.0,
    });
    cache.flat_disc = meshes.add(Cylinder::new(0.5, 0.04));
    cache.capsule_mesh = meshes.add(Capsule3d::new(0.28, 0.48));
    cache.hp_bg_mesh = meshes.add(Cuboid::new(1.0, 0.11, 0.04));
    cache.hp_fill_mesh = meshes.add(Cuboid::new(0.96, 0.08, 0.05));
    cache.bolt_mesh = meshes.add(Cuboid::new(0.11, 0.11, 0.48));
    cache.wing_mesh = meshes.add(Cuboid::new(0.9, 0.05, 0.36));

    cache.mat_mortar_dark = materials.add(StandardMaterial {
        base_color: Color::srgb(0.11, 0.12, 0.13),
        perceptual_roughness: 0.95,
        ..default()
    });
    cache.mat_shadow_disc = materials.add(StandardMaterial {
        base_color: Color::srgba(0.02, 0.02, 0.04, 0.62),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    cache.mat_ice_shell = materials.add(StandardMaterial {
        base_color: Color::srgba(0.52, 0.92, 1.0, 0.48),
        emissive: LinearRgba::rgb(0.2, 0.65, 1.0),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.12,
        ..default()
    });
    cache.mat_hp_bg = materials.add(StandardMaterial {
        base_color: Color::srgba(0.08, 0.08, 0.10, 0.90),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    cache.mat_hp_green = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.92, 0.28),
        emissive: LinearRgba::rgb(0.1, 0.5, 0.12),
        unlit: true,
        ..default()
    });
    cache.mat_hp_yellow = materials.add(StandardMaterial {
        base_color: Color::srgb(0.96, 0.82, 0.16),
        emissive: LinearRgba::rgb(0.5, 0.4, 0.05),
        unlit: true,
        ..default()
    });
    cache.mat_hp_red = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.18, 0.16),
        emissive: LinearRgba::rgb(0.55, 0.08, 0.06),
        unlit: true,
        ..default()
    });
    cache.mat_poison_pool = materials.add(StandardMaterial {
        base_color: Color::srgba(0.22, 0.88, 0.18, 0.68),
        emissive: LinearRgba::rgb(0.25, 1.15, 0.15),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.25,
        ..default()
    });
    cache.mat_enrage_aura = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.15, 0.10, 0.72),
        emissive: LinearRgba::rgb(2.2, 0.2, 0.1),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    cache.mat_loop_ring = materials.add(StandardMaterial {
        base_color: Color::srgba(0.75, 0.22, 1.0, 0.85),
        emissive: LinearRgba::rgb(1.4, 0.3, 2.2),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    cache.mat_reclaim_lock = materials.add(StandardMaterial {
        base_color: Color::srgba(0.92, 0.18, 0.18, 0.45),
        emissive: LinearRgba::rgb(0.8, 0.1, 0.1),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    cache.mat_gold = materials.add(StandardMaterial {
        base_color: Color::srgb(0.96, 0.78, 0.24),
        metallic: 0.88,
        perceptual_roughness: 0.26,
        emissive: LinearRgba::rgb(0.25, 0.16, 0.02),
        ..default()
    });
    cache.mat_dark_iron = materials.add(StandardMaterial {
        base_color: Color::srgb(0.24, 0.25, 0.28),
        metallic: 0.75,
        perceptual_roughness: 0.42,
        ..default()
    });
    cache.mat_wood = materials.add(StandardMaterial {
        base_color: Color::srgb(0.44, 0.28, 0.16),
        perceptual_roughness: 0.85,
        ..default()
    });
    cache.mat_bone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.79, 0.70),
        perceptual_roughness: 0.68,
        ..default()
    });
    cache.mat_obsidian = materials.add(StandardMaterial {
        base_color: Color::srgb(0.14, 0.11, 0.16),
        metallic: 0.35,
        perceptual_roughness: 0.35,
        ..default()
    });
    cache.mat_marble = materials.add(StandardMaterial {
        base_color: Color::srgb(0.88, 0.89, 0.92),
        perceptual_roughness: 0.32,
        metallic: 0.08,
        ..default()
    });
    cache.mat_stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.52, 0.53, 0.56),
        perceptual_roughness: 0.78,
        ..default()
    });
    cache.mat_glacier_stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.62, 0.82, 0.96),
        perceptual_roughness: 0.22,
        metallic: 0.15,
        ..default()
    });
    cache.mat_tree_trunk = materials.add(StandardMaterial {
        base_color: Color::srgb(0.32, 0.20, 0.12),
        perceptual_roughness: 0.92,
        ..default()
    });
    cache.mat_tree_pine_grass = materials.add(StandardMaterial {
        base_color: Color::srgb(0.14, 0.36, 0.16),
        perceptual_roughness: 0.85,
        ..default()
    });
    cache.mat_tree_pine_ice = materials.add(StandardMaterial {
        base_color: Color::srgb(0.28, 0.48, 0.56),
        perceptual_roughness: 0.75,
        ..default()
    });
    cache.mat_torch_flame = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.65, 0.15),
        emissive: LinearRgba::rgb(4.5, 2.1, 0.4),
        ..default()
    });
}

// ============================================================================
// Biome Lighting, Terrain Grid & Border Scenery
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn spawn_biome_and_map_visuals(
    mut commands: Commands,
    mut start_reader: MessageReader<StartMatchRequest>,
    grid: Res<GridMap>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut cache: ResMut<VisualCache>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    old_map_entities: Query<Entity, Or<(With<MapTileEntity>, With<BiomeVisualProp>)>>,
) {
    let start_requested = start_reader.read().count() > 0;
    let map_sig = (grid.map_id, grid.width, grid.height);
    let need_rebuild =
        grid.width > 0 && grid.height > 0 && (start_requested || cache.built_map_version != Some(map_sig));

    if !need_rebuild {
        return;
    }
    cache.built_map_version = Some(map_sig);

    // Clear previous biome lights, tiles & border props
    for e in &old_map_entities {
        if let Ok(mut ec) = commands.get_entity(e) {
            ec.despawn();
        }
    }

    // 1. Biome GlobalAmbientLight & DirectionalLight
    match grid.biome {
        BiomeKind::LordaeronGrass => {
            ambient.color = Color::srgb(1.0, 0.93, 0.78);
            ambient.brightness = 280.0;
        }
        BiomeKind::WintermaulIce => {
            ambient.color = Color::srgb(0.80, 0.90, 1.0);
            ambient.brightness = 320.0;
        }
    }

    let sun_color = match grid.biome {
        BiomeKind::LordaeronGrass => Color::srgb(1.0, 0.95, 0.84),
        BiomeKind::WintermaulIce => Color::srgb(0.88, 0.94, 1.0),
    };

    commands.spawn((
        DirectionalLight {
            illuminance: 14_000.0,
            color: sun_color,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.55, -0.85, 0.0)),
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 18.0,
            maximum_distance: 65.0,
            ..default()
        }
        .build(),
        GameplayEntity,
        BiomeVisualProp,
    ));

    // Subtle dark mortar base bed beneath all tiles so 0.96x0.96 tiles have crisp WC3 mortar seams
    let bed_w = grid.width as f32 * TILE_SIZE + 0.4;
    let bed_h = grid.height as f32 * TILE_SIZE + 0.4;
    commands.spawn((
        Mesh3d(cache.unit_cube.clone()),
        MeshMaterial3d(cache.mat_mortar_dark.clone()),
        Transform::from_xyz(0.0, -0.28, 0.0).with_scale(Vec3::new(bed_w, 0.16, bed_h)),
        Visibility::default(),
        GameplayEntity,
        BiomeVisualProp,
    ));

    // Outer floating fantasy arena stone rim
    let rim_color = match grid.biome {
        BiomeKind::LordaeronGrass => Color::srgb(0.32, 0.33, 0.35),
        BiomeKind::WintermaulIce => Color::srgb(0.38, 0.48, 0.58),
    };
    let rim_mat = cache.color_mat(&mut materials, rim_color, 0.85, 0.05, None);
    let arena_margin = 7.0;
    commands.spawn((
        Mesh3d(cache.unit_cube.clone()),
        MeshMaterial3d(rim_mat),
        Transform::from_xyz(0.0, -0.58, 0.0).with_scale(Vec3::new(
            bed_w + arena_margin,
            0.55,
            bed_h + arena_margin,
        )),
        Visibility::default(),
        GameplayEntity,
        BiomeVisualProp,
    ));

    // Pre-build biome tile materials
    let (tile_a_col, tile_b_col, blocked_col) = match grid.biome {
        BiomeKind::LordaeronGrass => (
            Color::srgb(0.24, 0.48, 0.21),
            Color::srgb(0.20, 0.42, 0.18),
            Color::srgb(0.42, 0.41, 0.39),
        ),
        BiomeKind::WintermaulIce => (
            Color::srgb(0.74, 0.86, 0.95),
            Color::srgb(0.64, 0.78, 0.90),
            Color::srgb(0.44, 0.64, 0.84),
        ),
    };
    let tile_mat_a = cache.color_mat(&mut materials, tile_a_col, 0.82, 0.02, None);
    let tile_mat_b = cache.color_mat(&mut materials, tile_b_col, 0.82, 0.02, None);
    let blocked_mat = cache.color_mat(&mut materials, blocked_col, 0.68, 0.08, None);
    let spawn_tile_mat = cache.color_mat(
        &mut materials,
        Color::srgb(0.24, 0.14, 0.28),
        0.55,
        0.15,
        Some(LinearRgba::rgb(0.35, 0.06, 0.42)),
    );
    let cp_tile_mat = cache.color_mat(
        &mut materials,
        Color::srgb(0.30, 0.34, 0.40),
        0.48,
        0.25,
        Some(LinearRgba::rgb(0.18, 0.28, 0.35)),
    );
    let goal_tile_mat = cache.color_mat(
        &mut materials,
        Color::srgb(0.18, 0.34, 0.44),
        0.35,
        0.30,
        Some(LinearRgba::rgb(0.25, 0.65, 0.95)),
    );

    // 2. Spawn 3D terrain tiles
    for z in 0..grid.height {
        for x in 0..grid.width {
            let coord = TileCoord::new(x, z);
            let kind = grid.get(coord).unwrap_or(TileKind::Buildable);
            let world = grid.tile_to_world(coord);
            let checker = (x + z) % 2 == 0;

            let mat = match kind {
                TileKind::Buildable | TileKind::Tower(_) => {
                    if checker {
                        tile_mat_a.clone()
                    } else {
                        tile_mat_b.clone()
                    }
                }
                TileKind::BlockedTerrain => blocked_mat.clone(),
                TileKind::Spawn(_) => spawn_tile_mat.clone(),
                TileKind::Checkpoint(_) => cp_tile_mat.clone(),
                TileKind::Goal => goal_tile_mat.clone(),
            };

            let y_pos = match kind {
                TileKind::Checkpoint(_) => -0.15,
                _ => -0.13,
            };

            commands.spawn((
                Mesh3d(cache.tile_mesh.clone()),
                MeshMaterial3d(mat),
                Transform::from_xyz(world.x, y_pos, world.z),
                Visibility::default(),
                MapTileEntity { coord },
                GameplayEntity,
            ));

            // Tall faceted crag / ice crystal boulder on BlockedTerrain
            if matches!(kind, TileKind::BlockedTerrain) {
                let crystal_mat = if grid.biome == BiomeKind::WintermaulIce {
                    cache.color_mat(
                        &mut materials,
                        Color::srgb(0.55, 0.85, 1.0),
                        0.18,
                        0.20,
                        Some(LinearRgba::rgb(0.1, 0.35, 0.65)),
                    )
                } else {
                    blocked_mat.clone()
                };
                commands
                    .spawn((
                        Transform::from_xyz(world.x, 0.0, world.z),
                        Visibility::default(),
                        GameplayEntity,
                        BiomeVisualProp,
                    ))
                    .with_children(|p| {
                        p.spawn((
                            Mesh3d(cache.crystal_frustum.clone()),
                            MeshMaterial3d(crystal_mat.clone()),
                            Transform::from_xyz(0.0, 0.35, 0.0)
                                .with_scale(Vec3::new(0.85, 0.72, 0.85))
                                .with_rotation(Quat::from_rotation_y(((x * 7 + z * 13) as f32) * 0.4)),
                        ));
                        p.spawn((
                            Mesh3d(cache.unit_cone.clone()),
                            MeshMaterial3d(crystal_mat),
                            Transform::from_xyz(0.14, 0.68, -0.10)
                                .with_scale(Vec3::new(0.45, 0.55, 0.45))
                                .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.15, 0.4, -0.2)),
                        ));
                    });
            }
        }
    }

    // 3. Spawn Zone Portals (Spawn archways, Checkpoint rune plates, Goal grand crystal portal)
    let spawn_rune_mat = cache.color_mat(
        &mut materials,
        Color::srgb(0.95, 0.25, 0.75),
        0.2,
        0.3,
        Some(LinearRgba::rgb(2.4, 0.35, 1.6)),
    );
    let cp_rune_mat = cache.color_mat(
        &mut materials,
        Color::srgb(0.98, 0.80, 0.25),
        0.2,
        0.4,
        Some(LinearRgba::rgb(2.1, 1.5, 0.25)),
    );
    let goal_core_mat = cache.color_mat(
        &mut materials,
        Color::srgb(0.32, 0.90, 1.0),
        0.15,
        0.2,
        Some(LinearRgba::rgb(0.8, 2.4, 3.4)),
    );

    for spawn_tiles in &grid.spawns {
        let c = grid.zone_center_world(spawn_tiles);
        spawn_archway_portal(
            &mut commands,
            &cache,
            c,
            cache.mat_obsidian.clone(),
            spawn_rune_mat.clone(),
            0.85,
            1.6,
        );
    }

    for (idx, cp_tiles) in grid.checkpoints.iter().enumerate() {
        let c = grid.zone_center_world(cp_tiles);
        let ring_mat = if idx % 2 == 0 {
            cp_rune_mat.clone()
        } else {
            goal_core_mat.clone()
        };
        commands.spawn((
            Mesh3d(cache.thin_torus.clone()),
            MeshMaterial3d(ring_mat),
            Transform::from_xyz(c.x, 0.04, c.z).with_scale(Vec3::new(1.5, 0.45, 1.5)),
            Visibility::default(),
            MapPortalRing {
                spin_speed: 1.4,
                base_y: 0.04,
                pulse_phase: idx as f32 * 1.1,
            },
            GameplayEntity,
            BiomeVisualProp,
        ));
    }

    if !grid.goals.is_empty() {
        let c = grid.zone_center_world(&grid.goals);
        let pillar_mat = if grid.biome == BiomeKind::WintermaulIce {
            cache.mat_glacier_stone.clone()
        } else {
            cache.mat_marble.clone()
        };
        spawn_archway_portal(
            &mut commands,
            &cache,
            c,
            pillar_mat,
            goal_core_mat,
            1.05,
            -2.0,
        );
    }

    // 4. Border scenery outside playable grid (pine trees, faceted rocks, 4 corner torches)
    let half_w = grid.width as f32 * 0.5;
    let half_h = grid.height as f32 * 0.5;
    let tree_foliage = if grid.biome == BiomeKind::WintermaulIce {
        cache.mat_tree_pine_ice.clone()
    } else {
        cache.mat_tree_pine_grass.clone()
    };

    // Place deterministic pine trees & rocks around perimeter
    let mut decor_idx = 0usize;
    let step = 3.0_f32;
    let mut x = -half_w - 2.0;
    while x <= half_w + 2.0 {
        for &z_side in &[-half_h - 1.8, half_h + 1.8] {
            let jitter_x = ((decor_idx * 37 % 10) as f32 - 5.0) * 0.09;
            let jitter_z = ((decor_idx * 53 % 10) as f32 - 5.0) * 0.08;
            if decor_idx.is_multiple_of(3) {
                spawn_rock_cluster(
                    &mut commands,
                    &cache,
                    Vec3::new(x + jitter_x, -0.1, z_side + jitter_z),
                    blocked_mat.clone(),
                    decor_idx,
                );
            } else {
                spawn_pine_tree(
                    &mut commands,
                    &cache,
                    Vec3::new(x + jitter_x, -0.15, z_side + jitter_z),
                    tree_foliage.clone(),
                    0.85 + (decor_idx % 4) as f32 * 0.14,
                );
            }
            decor_idx += 1;
        }
        x += step;
    }

    let mut z = -half_h + 1.0;
    while z <= half_h - 1.0 {
        for &x_side in &[-half_w - 1.9, half_w + 1.9] {
            let jitter_z = ((decor_idx * 41 % 10) as f32 - 5.0) * 0.09;
            if decor_idx % 3 == 1 {
                spawn_rock_cluster(
                    &mut commands,
                    &cache,
                    Vec3::new(x_side, -0.1, z + jitter_z),
                    blocked_mat.clone(),
                    decor_idx,
                );
            } else {
                spawn_pine_tree(
                    &mut commands,
                    &cache,
                    Vec3::new(x_side, -0.15, z + jitter_z),
                    tree_foliage.clone(),
                    0.9 + (decor_idx % 3) as f32 * 0.15,
                );
            }
            decor_idx += 1;
        }
        z += step;
    }

    // 4 flickering corner torches (PointLight shadow_maps_enabled: false)
    let corners = [
        Vec3::new(-half_w - 1.2, 0.0, -half_h - 1.2),
        Vec3::new(half_w + 1.2, 0.0, -half_h - 1.2),
        Vec3::new(-half_w - 1.2, 0.0, half_h + 1.2),
        Vec3::new(half_w + 1.2, 0.0, half_h + 1.2),
    ];
    for (i, corner) in corners.iter().enumerate() {
        spawn_corner_torch(&mut commands, &cache, *corner, i as f32 * 1.7);
    }
}

fn spawn_archway_portal(
    commands: &mut Commands,
    cache: &VisualCache,
    center: Vec3,
    pillar_mat: Handle<StandardMaterial>,
    core_mat: Handle<StandardMaterial>,
    scale: f32,
    spin_speed: f32,
) {
    commands
        .spawn((
            Transform::from_xyz(center.x, 0.0, center.z).with_scale(Vec3::splat(scale)),
            Visibility::default(),
            GameplayEntity,
            BiomeVisualProp,
        ))
        .with_children(|p| {
            // Left & right pillars
            for sx in [-0.72, 0.72] {
                p.spawn((
                    Mesh3d(cache.unit_cylinder.clone()),
                    MeshMaterial3d(pillar_mat.clone()),
                    Transform::from_xyz(sx, 0.75, 0.0).with_scale(Vec3::new(0.28, 1.5, 0.28)),
                ));
            }
            // Lintel arch top
            p.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(pillar_mat),
                Transform::from_xyz(0.0, 1.55, 0.0).with_scale(Vec3::new(1.75, 0.24, 0.34)),
            ));
            // Ground rune ring
            p.spawn((
                Mesh3d(cache.thin_torus.clone()),
                MeshMaterial3d(core_mat.clone()),
                Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(1.55, 0.5, 1.55)),
                MapPortalRing {
                    spin_speed,
                    base_y: 0.05,
                    pulse_phase: 0.0,
                },
            ));
            // Swirling emissive core inside arch
            p.spawn((
                Mesh3d(cache.unit_torus.clone()),
                MeshMaterial3d(core_mat),
                Transform::from_xyz(0.0, 0.85, 0.0)
                    .with_scale(Vec3::new(0.92, 0.92, 0.45))
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                MapPortalRing {
                    spin_speed: -spin_speed * 1.4,
                    base_y: 0.85,
                    pulse_phase: 1.5,
                },
            ));
        });
}

fn spawn_pine_tree(
    commands: &mut Commands,
    cache: &VisualCache,
    pos: Vec3,
    foliage_mat: Handle<StandardMaterial>,
    scale: f32,
) {
    commands
        .spawn((
            Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
            Visibility::default(),
            GameplayEntity,
            BiomeVisualProp,
        ))
        .with_children(|p| {
            p.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(cache.mat_tree_trunk.clone()),
                Transform::from_xyz(0.0, 0.35, 0.0).with_scale(Vec3::new(0.22, 0.7, 0.22)),
            ));
            p.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(foliage_mat.clone()),
                Transform::from_xyz(0.0, 0.95, 0.0).with_scale(Vec3::new(1.15, 0.95, 1.15)),
            ));
            p.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(foliage_mat.clone()),
                Transform::from_xyz(0.0, 1.45, 0.0).with_scale(Vec3::new(0.88, 0.85, 0.88)),
            ));
            p.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(foliage_mat),
                Transform::from_xyz(0.0, 1.90, 0.0).with_scale(Vec3::new(0.58, 0.75, 0.58)),
            ));
        });
}

fn spawn_rock_cluster(
    commands: &mut Commands,
    cache: &VisualCache,
    pos: Vec3,
    rock_mat: Handle<StandardMaterial>,
    seed: usize,
) {
    let yaw = (seed as f32) * 0.85;
    commands
        .spawn((
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
            GameplayEntity,
            BiomeVisualProp,
        ))
        .with_children(|p| {
            p.spawn((
                Mesh3d(cache.crystal_frustum.clone()),
                MeshMaterial3d(rock_mat.clone()),
                Transform::from_xyz(0.0, 0.32, 0.0).with_scale(Vec3::new(1.05, 0.65, 0.92)),
            ));
            p.spawn((
                Mesh3d(cache.crystal_frustum.clone()),
                MeshMaterial3d(rock_mat),
                Transform::from_xyz(0.35, 0.22, 0.22)
                    .with_scale(Vec3::new(0.60, 0.45, 0.55))
                    .with_rotation(Quat::from_rotation_z(0.25)),
            ));
        });
}

fn spawn_corner_torch(commands: &mut Commands, cache: &VisualCache, pos: Vec3, phase: f32) {
    commands
        .spawn((
            Transform::from_translation(pos),
            Visibility::default(),
            GameplayEntity,
            BiomeVisualProp,
        ))
        .with_children(|p| {
            p.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(cache.mat_dark_iron.clone()),
                Transform::from_xyz(0.0, 0.65, 0.0).with_scale(Vec3::new(0.18, 1.3, 0.18)),
            ));
            p.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(cache.mat_torch_flame.clone()),
                Transform::from_xyz(0.0, 1.48, 0.0).with_scale(Vec3::new(0.32, 0.46, 0.32)),
            ));
            p.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.62, 0.22),
                    intensity: 42_000.0,
                    range: 14.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(0.0, 1.65, 0.0),
                FlickeringTorchLight {
                    base_intensity: 42_000.0,
                    phase,
                },
            ));
        });
}

// ============================================================================
// Procedural 3D PBR Towers for ALL 5 Factions (41 Recipes)
// ============================================================================

fn spawn_tower_visuals(
    mut commands: Commands,
    mut towers: Query<(Entity, &mut Tower), Without<PendingDespawn>>,
    configs: Res<GameConfigs>,
    mut cache: ResMut<VisualCache>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (tower_entity, mut tower) in &mut towers {
        if tower.visual_spawned {
            continue;
        }
        tower.visual_spawned = true;

        let Some(cfg) = configs.get_tower(tower.faction, &tower.id).cloned() else {
            continue;
        };

        let Ok(mut ec) = commands.get_entity(tower_entity) else {
            continue;
        };
        // Despawn old children on upgrade so upgraded mesh replaces previous tier cleanly
        ec.despawn_children();

        let (p_r, p_g, p_b) = configs
            .get_faction(tower.faction)
            .map(|f| f.primary_color)
            .unwrap_or((0.7, 0.7, 0.7));
        let (proj_r, proj_g, proj_b) = cfg.projectile_color;
        let faction_col = Color::srgb(p_r, p_g, p_b);
        let accent_col = Color::srgb(proj_r, proj_g, proj_b);
        let emissive_glow = LinearRgba::rgb(proj_r * 2.2, proj_g * 2.2, proj_b * 2.2);

        let faction_mat = cache.color_mat(&mut materials, faction_col, 0.45, 0.25, None);
        let accent_mat = cache.color_mat(&mut materials, accent_col, 0.22, 0.20, Some(emissive_glow));
        let recipe = cfg.mesh_recipe;
        let tier = cfg.tier;

        ec.with_children(|parent| {
            // 1. Faction Architectural Pedestal / Base
            let turret_y = spawn_faction_pedestal(parent, &cache, tower.faction, tier, &faction_mat);

            // 2. Rotating Turret Head (recoils & slerps toward tower.target)
            let head_offset = Vec3::new(0.0, turret_y, 0.0);
            parent
                .spawn((
                    Transform::from_translation(head_offset),
                    Visibility::default(),
                    TowerTurretHead {
                        tower: tower_entity,
                        base_offset: head_offset,
                    },
                ))
                .with_children(|head| {
                    spawn_recipe_turret_head(
                        head,
                        &mut cache,
                        &mut materials,
                        recipe,
                        tier,
                        faction_mat.clone(),
                        accent_mat.clone(),
                    );
                });

            // 3. Floating Spinning TowerVisualAccent (unmistakable from isometric camera)
            spawn_recipe_accent(
                parent,
                &mut cache,
                &mut materials,
                tower_entity,
                recipe,
                tier,
                turret_y,
                accent_mat,
            );
        });
    }
}

fn spawn_faction_pedestal(
    parent: &mut ChildSpawnerCommands<'_>,
    cache: &VisualCache,
    faction: FactionId,
    tier: u8,
    faction_mat: &Handle<StandardMaterial>,
) -> f32 {
    let tier_h = match tier {
        1 => 1.0,
        2 => 1.24,
        _ => 1.52,
    };

    match faction {
        FactionId::ElementalCircle => {
            // White marble stepped octagonal/square pillar
            parent.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_marble.clone()),
                Transform::from_xyz(0.0, 0.12, 0.0).with_scale(Vec3::new(0.82, 0.24, 0.82)),
            ));
            parent.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(cache.mat_marble.clone()),
                Transform::from_xyz(0.0, 0.24 + 0.34 * tier_h, 0.0)
                    .with_scale(Vec3::new(0.56, 0.68 * tier_h, 0.56)),
            ));
            parent.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(faction_mat.clone()),
                Transform::from_xyz(0.0, 0.26 + 0.68 * tier_h, 0.0)
                    .with_scale(Vec3::new(0.66, 0.10, 0.66)),
            ));
            0.35 + 0.68 * tier_h
        }
        FactionId::Citadel => {
            // Grey medieval stone keep + wood crenellations
            parent.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_stone.clone()),
                Transform::from_xyz(0.0, 0.36 * tier_h, 0.0)
                    .with_scale(Vec3::new(0.76, 0.72 * tier_h, 0.76)),
            ));
            parent.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_wood.clone()),
                Transform::from_xyz(0.0, 0.74 * tier_h, 0.0).with_scale(Vec3::new(0.84, 0.12, 0.84)),
            ));
            if tier >= 2 {
                for (cx, cz) in [(-0.34, -0.34), (0.34, -0.34), (-0.34, 0.34), (0.34, 0.34)] {
                    parent.spawn((
                        Mesh3d(cache.unit_cube.clone()),
                        MeshMaterial3d(cache.mat_stone.clone()),
                        Transform::from_xyz(cx, 0.84 * tier_h, cz)
                            .with_scale(Vec3::new(0.14, 0.14, 0.14)),
                    ));
                }
            }
            0.84 * tier_h
        }
        FactionId::DragonClan => {
            // Obsidian basalt plinth with magma trim
            parent.spawn((
                Mesh3d(cache.crystal_frustum.clone()),
                MeshMaterial3d(cache.mat_obsidian.clone()),
                Transform::from_xyz(0.0, 0.40 * tier_h, 0.0)
                    .with_scale(Vec3::new(0.88, 0.80 * tier_h, 0.88)),
            ));
            parent.spawn((
                Mesh3d(cache.thin_torus.clone()),
                MeshMaterial3d(faction_mat.clone()),
                Transform::from_xyz(0.0, 0.22, 0.0).with_scale(Vec3::new(0.82, 0.35, 0.82)),
            ));
            0.82 * tier_h
        }
        FactionId::Frostguard => {
            // Sharp faceted glacier crystal pedestal
            parent.spawn((
                Mesh3d(cache.crystal_frustum.clone()),
                MeshMaterial3d(cache.mat_glacier_stone.clone()),
                Transform::from_xyz(0.0, 0.44 * tier_h, 0.0)
                    .with_scale(Vec3::new(0.80, 0.88 * tier_h, 0.80)),
            ));
            for i in 0..4 {
                let ang = i as f32 * std::f32::consts::FRAC_PI_2;
                parent.spawn((
                    Mesh3d(cache.unit_cone.clone()),
                    MeshMaterial3d(faction_mat.clone()),
                    Transform::from_xyz(ang.cos() * 0.32, 0.26, ang.sin() * 0.32)
                        .with_scale(Vec3::new(0.18, 0.46, 0.18)),
                ));
            }
            0.88 * tier_h
        }
        FactionId::Necropolis => {
            // Stepped dark obsidian & bone ziggurat
            parent.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_obsidian.clone()),
                Transform::from_xyz(0.0, 0.14, 0.0).with_scale(Vec3::new(0.84, 0.28, 0.84)),
            ));
            parent.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_obsidian.clone()),
                Transform::from_xyz(0.0, 0.28 + 0.26 * tier_h, 0.0)
                    .with_scale(Vec3::new(0.62, 0.52 * tier_h, 0.62)),
            ));
            for (bx, bz) in [(-0.34, -0.34), (0.34, -0.34), (-0.34, 0.34), (0.34, 0.34)] {
                parent.spawn((
                    Mesh3d(cache.unit_cone.clone()),
                    MeshMaterial3d(cache.mat_bone.clone()),
                    Transform::from_xyz(bx, 0.34, bz).with_scale(Vec3::new(0.12, 0.42, 0.12)),
                ));
            }
            0.34 + 0.52 * tier_h
        }
    }
}

fn spawn_recipe_turret_head(
    head: &mut ChildSpawnerCommands<'_>,
    cache: &mut VisualCache,
    materials: &mut Assets<StandardMaterial>,
    recipe: TowerMeshRecipe,
    tier: u8,
    faction_mat: Handle<StandardMaterial>,
    accent_mat: Handle<StandardMaterial>,
) {
    match recipe {
        // --- Elemental Circle ---
        TowerMeshRecipe::ArcanePillar => {
            head.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(faction_mat),
                Transform::from_xyz(0.0, 0.18, 0.0).with_scale(Vec3::new(0.32, 0.45, 0.32)),
            ));
            head.spawn((
                Mesh3d(cache.bolt_mesh.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.16, -0.22),
            ));
        }
        TowerMeshRecipe::FireObelisk | TowerMeshRecipe::InfernoSpire => {
            let s = if tier >= 3 { 1.32 } else { 1.0 };
            head.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(cache.mat_obsidian.clone()),
                Transform::from_xyz(0.0, 0.22 * s, 0.0).with_scale(Vec3::new(0.38 * s, 0.55 * s, 0.38 * s)),
            ));
            head.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.18 * s, -0.24 * s)
                    .with_scale(Vec3::new(0.16 * s, 0.38 * s, 0.16 * s))
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ));
        }
        TowerMeshRecipe::WaterFountain | TowerMeshRecipe::GlacierSource => {
            let s = if tier >= 3 { 1.28 } else { 1.0 };
            head.spawn((
                Mesh3d(cache.unit_torus.clone()),
                MeshMaterial3d(cache.mat_marble.clone()),
                Transform::from_xyz(0.0, 0.06, 0.0).with_scale(Vec3::new(0.60 * s, 0.45, 0.60 * s)),
            ));
            head.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.24 * s, -0.18)
                    .with_scale(Vec3::new(0.22 * s, 0.42 * s, 0.22 * s))
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            ));
        }
        TowerMeshRecipe::LightningCoil | TowerMeshRecipe::ThunderPylon => {
            let s = if tier >= 3 { 1.30 } else { 1.0 };
            head.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(cache.mat_dark_iron.clone()),
                Transform::from_xyz(0.0, 0.26 * s, 0.0).with_scale(Vec3::new(0.20, 0.55 * s, 0.20)),
            ));
            for i in 0..2 {
                head.spawn((
                    Mesh3d(cache.thin_torus.clone()),
                    MeshMaterial3d(accent_mat.clone()),
                    Transform::from_xyz(0.0, (0.20 + i as f32 * 0.20) * s, 0.0)
                        .with_scale(Vec3::new(0.46 * s, 0.45, 0.46 * s)),
                ));
            }
        }
        TowerMeshRecipe::SteamCondenser => {
            let copper = cache.color_mat(materials, Color::srgb(0.82, 0.54, 0.30), 0.3, 0.85, None);
            head.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(copper.clone()),
                Transform::from_xyz(0.0, 0.16, 0.0).with_scale(Vec3::splat(0.46)),
            ));
            // Twin steam vents
            for sx in [-0.20, 0.20] {
                head.spawn((
                    Mesh3d(cache.unit_cylinder.clone()),
                    MeshMaterial3d(copper.clone()),
                    Transform::from_xyz(sx, 0.34, -0.05)
                        .with_scale(Vec3::new(0.14, 0.42, 0.14))
                        .with_rotation(Quat::from_rotation_x(-0.3)),
                ));
            }
            head.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.46, -0.15).with_scale(Vec3::splat(0.24)),
            ));
        }
        TowerMeshRecipe::PlasmaCore => {
            let gold_emissive = cache.color_mat(
                materials,
                Color::srgb(1.0, 0.85, 0.30),
                0.2,
                0.7,
                Some(LinearRgba::rgb(2.2, 1.5, 0.2)),
            );
            head.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.28, 0.0).with_scale(Vec3::splat(0.42)),
            ));
            head.spawn((
                Mesh3d(cache.thin_torus.clone()),
                MeshMaterial3d(gold_emissive),
                Transform::from_xyz(0.0, 0.28, 0.0)
                    .with_scale(Vec3::splat(0.68))
                    .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.9, 0.4, 0.2)),
            ));
        }
        TowerMeshRecipe::StormPillar => {
            head.spawn((
                Mesh3d(cache.crystal_frustum.clone()),
                MeshMaterial3d(faction_mat),
                Transform::from_xyz(0.0, 0.26, 0.0).with_scale(Vec3::new(0.46, 0.56, 0.46)),
            ));
            head.spawn((
                Mesh3d(cache.thin_torus.clone()),
                MeshMaterial3d(accent_mat.clone()),
                Transform::from_xyz(0.0, 0.26, 0.0)
                    .with_scale(Vec3::splat(0.64))
                    .with_rotation(Quat::from_rotation_x(0.45)),
            ));
            head.spawn((
                Mesh3d(cache.thin_torus.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.36, 0.0)
                    .with_scale(Vec3::splat(0.54))
                    .with_rotation(Quat::from_rotation_z(-0.45)),
            ));
        }

        // --- Citadel ---
        TowerMeshRecipe::Watchtower => {
            head.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_wood.clone()),
                Transform::from_xyz(0.0, 0.12, 0.0).with_scale(Vec3::new(0.52, 0.10, 0.16)),
            ));
            head.spawn((
                Mesh3d(cache.bolt_mesh.clone()),
                MeshMaterial3d(cache.mat_gold.clone()),
                Transform::from_xyz(0.0, 0.14, -0.18).with_scale(Vec3::new(1.0, 1.0, 1.15)),
            ));
        }
        TowerMeshRecipe::RapidFireTower | TowerMeshRecipe::GatlingBallista => {
            let barrel_count = if matches!(recipe, TowerMeshRecipe::GatlingBallista) {
                3
            } else {
                2
            };
            head.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_dark_iron.clone()),
                Transform::from_xyz(0.0, 0.12, 0.04).with_scale(Vec3::new(0.36, 0.22, 0.36)),
            ));
            for i in 0..barrel_count {
                let ox = (i as f32 - (barrel_count - 1) as f32 * 0.5) * 0.14;
                head.spawn((
                    Mesh3d(cache.unit_cylinder.clone()),
                    MeshMaterial3d(cache.mat_gold.clone()),
                    Transform::from_xyz(ox, 0.14, -0.28)
                        .with_scale(Vec3::new(0.08, 0.52, 0.08))
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ));
            }
        }
        TowerMeshRecipe::MultishotTower | TowerMeshRecipe::VolleyBastion => {
            let fan = if matches!(recipe, TowerMeshRecipe::VolleyBastion) {
                5
            } else {
                3
            };
            head.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_wood.clone()),
                Transform::from_xyz(0.0, 0.12, 0.0).with_scale(Vec3::new(0.68, 0.10, 0.22)),
            ));
            for i in 0..fan {
                let yaw = (i as f32 - (fan - 1) as f32 * 0.5) * 0.22;
                head.spawn((
                    Mesh3d(cache.bolt_mesh.clone()),
                    MeshMaterial3d(cache.mat_gold.clone()),
                    Transform::from_xyz(yaw.sin() * 0.2, 0.16, -0.22)
                        .with_rotation(Quat::from_rotation_y(-yaw)),
                ));
            }
        }
        TowerMeshRecipe::HeavyCannon | TowerMeshRecipe::SiegeMortar => {
            let is_mortar = matches!(recipe, TowerMeshRecipe::SiegeMortar);
            let tilt = if is_mortar { 0.85 } else { 1.38 };
            let scale_b = if is_mortar { 1.35 } else { 1.0 };
            head.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_wood.clone()),
                Transform::from_xyz(0.0, 0.12, 0.0).with_scale(Vec3::new(0.46, 0.20, 0.44)),
            ));
            head.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(cache.mat_dark_iron.clone()),
                Transform::from_xyz(0.0, 0.24, -0.18)
                    .with_scale(Vec3::new(0.26 * scale_b, 0.62 * scale_b, 0.26 * scale_b))
                    .with_rotation(Quat::from_rotation_x(tilt)),
            ));
        }
        TowerMeshRecipe::FlakHunter => {
            head.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(cache.mat_dark_iron.clone()),
                Transform::from_xyz(-0.14, 0.24, -0.06)
                    .with_scale(Vec3::new(0.11, 0.60, 0.11))
                    .with_rotation(Quat::from_rotation_x(0.45)),
            ));
            head.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(cache.mat_dark_iron.clone()),
                Transform::from_xyz(0.14, 0.24, -0.06)
                    .with_scale(Vec3::new(0.11, 0.60, 0.11))
                    .with_rotation(Quat::from_rotation_x(0.45)),
            ));
        }
        TowerMeshRecipe::GuildTreasury => {
            head.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(cache.mat_gold.clone()),
                Transform::from_xyz(0.0, 0.18, 0.0).with_scale(Vec3::new(0.65, 0.45, 0.65)),
            ));
        }

        // --- Dragon Clan ---
        TowerMeshRecipe::BroodNest
        | TowerMeshRecipe::RedDragonRoost
        | TowerMeshRecipe::ElderFireDragon
        | TowerMeshRecipe::BronzeDragonSpire
        | TowerMeshRecipe::TimeDragonSanctum
        | TowerMeshRecipe::StormDragonPerch
        | TowerMeshRecipe::SkyWyrmPeak => {
            let s = match tier {
                1 => 0.82,
                2 => 1.05,
                _ => 1.32,
            };
            // Dragon head + snout + horns + swept wings
            head.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(faction_mat.clone()),
                Transform::from_xyz(0.0, 0.18 * s, -0.10 * s)
                    .with_scale(Vec3::new(0.28 * s, 0.24 * s, 0.46 * s)),
            ));
            head.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(accent_mat.clone()),
                Transform::from_xyz(0.0, 0.16 * s, -0.40 * s)
                    .with_scale(Vec3::new(0.18 * s, 0.26 * s, 0.18 * s))
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            ));
            // Swept wings
            for sx in [-1.0_f32, 1.0_f32] {
                head.spawn((
                    Mesh3d(cache.wing_mesh.clone()),
                    MeshMaterial3d(faction_mat.clone()),
                    Transform::from_xyz(sx * 0.42 * s, 0.24 * s, 0.08 * s)
                        .with_scale(Vec3::new(0.65 * s, 1.0, 1.15 * s))
                        .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.2, sx * -0.35, sx * 0.35)),
                ));
            }
        }
        TowerMeshRecipe::GoldHoardShrine => {
            head.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(cache.mat_gold.clone()),
                Transform::from_xyz(0.0, 0.16, 0.0).with_scale(Vec3::new(0.68, 0.35, 0.68)),
            ));
        }

        // --- Frostguard ---
        TowerMeshRecipe::IcePillar => {
            head.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.20, 0.0).with_scale(Vec3::new(0.30, 0.50, 0.30)),
            ));
        }
        TowerMeshRecipe::FrostSpearThrower | TowerMeshRecipe::GlacierSniper => {
            let s = if tier >= 3 { 1.3 } else { 1.0 };
            head.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_glacier_stone.clone()),
                Transform::from_xyz(0.0, 0.12, 0.0).with_scale(Vec3::new(0.34, 0.18, 0.40)),
            ));
            head.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.16, -0.32 * s)
                    .with_scale(Vec3::new(0.14, 0.62 * s, 0.14))
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            ));
        }
        TowerMeshRecipe::BlizzardObelisk | TowerMeshRecipe::BlizzardSpire => {
            let s = if tier >= 3 { 1.32 } else { 1.0 };
            head.spawn((
                Mesh3d(cache.crystal_frustum.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.26 * s, 0.0)
                    .with_scale(Vec3::new(0.38 * s, 0.58 * s, 0.38 * s)),
            ));
        }
        TowerMeshRecipe::IceShardCatapult => {
            head.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_wood.clone()),
                Transform::from_xyz(0.0, 0.10, 0.0).with_scale(Vec3::new(0.48, 0.14, 0.52)),
            ));
            head.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.22, -0.10)
                    .with_scale(Vec3::new(0.12, 0.12, 0.56))
                    .with_rotation(Quat::from_rotation_x(0.45)),
            ));
        }
        TowerMeshRecipe::NorthwindTower => {
            for i in 0..3 {
                let a = i as f32 * std::f32::consts::TAU / 3.0;
                head.spawn((
                    Mesh3d(cache.wing_mesh.clone()),
                    MeshMaterial3d(accent_mat.clone()),
                    Transform::from_xyz(a.cos() * 0.22, 0.24, a.sin() * 0.22)
                        .with_scale(Vec3::new(0.42, 0.8, 0.45))
                        .with_rotation(Quat::from_rotation_y(-a)),
                ));
            }
        }

        // --- Necropolis ---
        TowerMeshRecipe::BoneSpire => {
            head.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(cache.mat_bone.clone()),
                Transform::from_xyz(0.0, 0.20, 0.0).with_scale(Vec3::new(0.28, 0.48, 0.28)),
            ));
        }
        TowerMeshRecipe::PlagueZiggurat | TowerMeshRecipe::PestCathedral => {
            let s = if tier >= 3 { 1.30 } else { 1.0 };
            head.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(cache.mat_obsidian.clone()),
                Transform::from_xyz(0.0, 0.14 * s, 0.0).with_scale(Vec3::new(0.54 * s, 0.26, 0.54 * s)),
            ));
            head.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.24 * s, 0.0).with_scale(Vec3::splat(0.38 * s)),
            ));
        }
        TowerMeshRecipe::DecayObelisk | TowerMeshRecipe::LichThron => {
            let s = if tier >= 3 { 1.32 } else { 1.0 };
            head.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(cache.mat_obsidian.clone()),
                Transform::from_xyz(0.0, 0.28 * s, 0.0)
                    .with_scale(Vec3::new(0.36 * s, 0.62 * s, 0.36 * s)),
            ));
            head.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.32 * s, -0.22)
                    .with_scale(Vec3::splat(0.20 * s)),
            ));
        }
        TowerMeshRecipe::BansheeTower => {
            head.spawn((
                Mesh3d(cache.capsule_mesh.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.26, 0.0).with_scale(Vec3::splat(0.65)),
            ));
        }
        TowerMeshRecipe::SoulCrypt => {
            head.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(cache.mat_bone.clone()),
                Transform::from_xyz(0.0, 0.18, 0.0).with_scale(Vec3::new(0.44, 0.30, 0.44)),
            ));
            head.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, 0.40, 0.0).with_scale(Vec3::splat(0.28)),
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_recipe_accent(
    parent: &mut ChildSpawnerCommands<'_>,
    cache: &mut VisualCache,
    materials: &mut Assets<StandardMaterial>,
    tower_entity: Entity,
    recipe: TowerMeshRecipe,
    tier: u8,
    turret_y: f32,
    accent_mat: Handle<StandardMaterial>,
) {
    let base_y = turret_y + 0.56 + (tier.saturating_sub(1) as f32) * 0.16;
    let spin_speed = match tier {
        1 => 1.2,
        2 => 2.1,
        _ => 3.0,
    };

    match recipe {
        TowerMeshRecipe::GuildTreasury | TowerMeshRecipe::GoldHoardShrine => {
            // Spinning golden treasury emblem
            parent.spawn((
                Mesh3d(cache.unit_torus.clone()),
                MeshMaterial3d(cache.mat_gold.clone()),
                Transform::from_xyz(0.0, base_y, 0.0).with_scale(Vec3::new(0.46, 0.46, 0.46)),
                TowerVisualAccent {
                    tower: tower_entity,
                    spin_speed: 1.8,
                    bob_amp: 0.05,
                    base_y,
                },
            ));
        }
        TowerMeshRecipe::PlasmaCore => {
            let magenta_mat = cache.color_mat(
                materials,
                Color::srgb(0.98, 0.25, 0.92),
                0.15,
                0.2,
                Some(LinearRgba::rgb(2.8, 0.4, 2.6)),
            );
            parent.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(magenta_mat),
                Transform::from_xyz(0.0, base_y, 0.0).with_scale(Vec3::splat(0.32)),
                TowerVisualAccent {
                    tower: tower_entity,
                    spin_speed: 3.4,
                    bob_amp: 0.09,
                    base_y,
                },
            ));
        }
        TowerMeshRecipe::StormPillar | TowerMeshRecipe::ThunderPylon | TowerMeshRecipe::LightningCoil => {
            parent.spawn((
                Mesh3d(cache.thin_torus.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, base_y, 0.0).with_scale(Vec3::new(0.56, 0.56, 0.56)),
                TowerVisualAccent {
                    tower: tower_entity,
                    spin_speed: 3.6,
                    bob_amp: 0.06,
                    base_y,
                },
            ));
        }
        TowerMeshRecipe::IcePillar
        | TowerMeshRecipe::FrostSpearThrower
        | TowerMeshRecipe::GlacierSniper
        | TowerMeshRecipe::BlizzardObelisk
        | TowerMeshRecipe::BlizzardSpire
        | TowerMeshRecipe::IceShardCatapult
        | TowerMeshRecipe::NorthwindTower => {
            // Floating ice octahedron-like double cone
            parent
                .spawn((
                    Transform::from_xyz(0.0, base_y, 0.0),
                    Visibility::default(),
                    TowerVisualAccent {
                        tower: tower_entity,
                        spin_speed,
                        bob_amp: 0.07,
                        base_y,
                    },
                ))
                .with_children(|p| {
                    p.spawn((
                        Mesh3d(cache.unit_cone.clone()),
                        MeshMaterial3d(accent_mat.clone()),
                        Transform::from_xyz(0.0, 0.12, 0.0).with_scale(Vec3::new(0.24, 0.24, 0.24)),
                    ));
                    p.spawn((
                        Mesh3d(cache.unit_cone.clone()),
                        MeshMaterial3d(accent_mat),
                        Transform::from_xyz(0.0, -0.12, 0.0)
                            .with_scale(Vec3::new(0.24, 0.24, 0.24))
                            .with_rotation(Quat::from_rotation_x(std::f32::consts::PI)),
                    ));
                });
        }
        TowerMeshRecipe::BoneSpire
        | TowerMeshRecipe::PlagueZiggurat
        | TowerMeshRecipe::PestCathedral
        | TowerMeshRecipe::DecayObelisk
        | TowerMeshRecipe::LichThron
        | TowerMeshRecipe::BansheeTower
        | TowerMeshRecipe::SoulCrypt => {
            // Floating green/violet soul lantern
            parent.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, base_y, 0.0).with_scale(Vec3::splat(0.22 + tier as f32 * 0.04)),
                TowerVisualAccent {
                    tower: tower_entity,
                    spin_speed,
                    bob_amp: 0.08,
                    base_y,
                },
            ));
        }
        _ => {
            // Elemental / Dragon / Citadel floating crystal/orb accent
            parent.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(accent_mat),
                Transform::from_xyz(0.0, base_y, 0.0).with_scale(Vec3::splat(0.18 + tier as f32 * 0.05)),
                TowerVisualAccent {
                    tower: tower_entity,
                    spin_speed,
                    bob_amp: 0.06,
                    base_y,
                },
            ));
        }
    }
}

fn animate_towers(
    time: Res<Time>,
    mut heads: Query<(&TowerTurretHead, &mut Transform), Without<Tower>>,
    mut accents: Query<(&TowerVisualAccent, &mut Transform), (Without<TowerTurretHead>, Without<Tower>)>,
    towers: Query<(&Tower, &GlobalTransform), Without<PendingDespawn>>,
    targets: Query<&GlobalTransform, Without<PendingDespawn>>,
) {
    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();

    for (head, mut tf) in &mut heads {
        let Ok((tower, tower_gtf)) = towers.get(head.tower) else {
            continue;
        };

        // Smooth slerp toward target
        if let Some(target_entity) = tower.target
            && let Ok(target_gtf) = targets.get(target_entity)
        {
            let tower_pos = tower_gtf.translation();
            let target_pos = target_gtf.translation();
            let dir = Vec3::new(target_pos.x - tower_pos.x, 0.0, target_pos.z - tower_pos.z);
            if dir.length_squared() > 0.0001 {
                let desired = Transform::IDENTITY.looking_to(dir.normalize(), Vec3::Y).rotation;
                tf.rotation = tf.rotation.slerp(desired, (dt * 12.0).clamp(0.0, 1.0));
            }
        }

        // Recoil kick along local +Z (backward from -Z forward)
        let recoil_vec = tf.rotation * Vec3::new(0.0, 0.0, tower.recoil * 0.18);
        tf.translation = head.base_offset + recoil_vec;
    }

    for (accent, mut tf) in &mut accents {
        if towers.get(accent.tower).is_err() {
            continue;
        }
        tf.rotate_y(accent.spin_speed * dt);
        tf.translation.y = accent.base_y + (elapsed * 2.4).sin() * accent.bob_amp;
    }
}

// ============================================================================
// Procedural 3D PBR Enemies, Animations, Status Effects & 3D Health Bars
// ============================================================================

fn spawn_enemy_visuals(
    mut commands: Commands,
    mut enemies: Query<(Entity, &mut Enemy), Without<PendingDespawn>>,
    configs: Res<GameConfigs>,
    mut cache: ResMut<VisualCache>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (enemy_entity, mut enemy) in &mut enemies {
        if enemy.visual_spawned {
            continue;
        }
        enemy.visual_spawned = true;

        let cfg = configs.get_enemy(enemy.archetype);
        let (pr, pg, pb) = cfg.map(|c| c.primary_color).unwrap_or((0.5, 0.5, 0.5));
        let (ar, ag, ab) = cfg.map(|c| c.accent_color).unwrap_or((0.9, 0.2, 0.2));

        let primary_col = Color::srgb(pr, pg, pb);
        let accent_col = Color::srgb(ar, ag, ab);
        let primary_mat = cache.color_mat(&mut materials, primary_col, 0.55, 0.12, None);
        let accent_mat = cache.color_mat(
            &mut materials,
            accent_col,
            0.28,
            0.25,
            Some(LinearRgba::rgb(ar * 1.4, ag * 1.4, ab * 1.4)),
        );

        let Ok(mut ec) = commands.get_entity(enemy_entity) else {
            continue;
        };

        let arch = enemy.archetype;
        let is_air = enemy.domain == MovementDomain::Air;
        let is_boss = enemy.is_boss;
        let scale = enemy.scale;

        let anim_kind = match arch {
            EnemyArchetypeId::RatSwarm
            | EnemyArchetypeId::GoblinRaider
            | EnemyArchetypeId::BossRatKing => EnemyAnimKind::Hop,
            EnemyArchetypeId::BatFlyer
            | EnemyArchetypeId::Gargoyle
            | EnemyArchetypeId::BossObsidianWing => EnemyAnimKind::FlyBob,
            EnemyArchetypeId::SlimeSplitter
            | EnemyArchetypeId::MiniSlime
            | EnemyArchetypeId::BossSlimeTitan => EnemyAnimKind::SquashStretch,
            EnemyArchetypeId::VampireLord
            | EnemyArchetypeId::BossBloodCount
            | EnemyArchetypeId::BossAetherDreadlord => EnemyAnimKind::FloatSway,
            EnemyArchetypeId::StoneGolem | EnemyArchetypeId::BossGolemColossus => {
                EnemyAnimKind::HeavyStomp
            }
        };

        ec.with_children(|parent| {
            // Ground shadow disc directly beneath flying enemies so 2D maze position is crystal clear
            if is_air {
                parent.spawn((
                    Mesh3d(cache.flat_disc.clone()),
                    MeshMaterial3d(cache.mat_shadow_disc.clone()),
                    Transform::from_xyz(0.0, 0.03 - AIR_ALTITUDE, 0.0)
                        .with_scale(Vec3::new(0.85 * scale, 1.0, 0.85 * scale)),
                ));
            }

            // Animated body root
            parent
                .spawn((
                    Transform::IDENTITY.with_scale(Vec3::splat(scale)),
                    Visibility::default(),
                    EnemyAnimBody {
                        enemy: enemy_entity,
                        kind: anim_kind,
                        base_y: 0.0,
                        scale,
                    },
                ))
                .with_children(|body| {
                    spawn_enemy_archetype_mesh(
                        body,
                        &mut cache,
                        &mut materials,
                        enemy_entity,
                        arch,
                        is_boss,
                        primary_mat,
                        accent_mat,
                    );
                });

            // Status effect overlays (hidden until active)
            parent.spawn((
                Mesh3d(cache.crystal_frustum.clone()),
                MeshMaterial3d(cache.mat_ice_shell.clone()),
                Transform::from_xyz(0.0, 0.45 * scale, 0.0)
                    .with_scale(Vec3::new(0.95 * scale, 0.95 * scale, 0.95 * scale)),
                Visibility::Hidden,
                EnemyFreezeShell {
                    enemy: enemy_entity,
                },
            ));
            parent.spawn((
                Mesh3d(cache.thin_torus.clone()),
                MeshMaterial3d(cache.mat_enrage_aura.clone()),
                Transform::from_xyz(0.0, 0.14, 0.0).with_scale(Vec3::new(0.9 * scale, 0.4, 0.9 * scale)),
                Visibility::Hidden,
                EnemyEnrageTint {
                    enemy: enemy_entity,
                },
            ));
            parent.spawn((
                Mesh3d(cache.thin_torus.clone()),
                MeshMaterial3d(cache.mat_loop_ring.clone()),
                Transform::from_xyz(0.0, if is_air { 0.06 - AIR_ALTITUDE } else { 0.06 }, 0.0)
                    .with_scale(Vec3::new(1.05 * scale, 0.4, 1.05 * scale)),
                Visibility::Hidden,
                EnemyLoopRing {
                    enemy: enemy_entity,
                },
            ));

            // 3D Billboard Health Bar
            let bar_w = if is_boss { 1.45 } else { 0.72 * scale.max(0.75) };
            let y_offset = 1.25 * scale;
            parent
                .spawn((
                    Mesh3d(cache.hp_bg_mesh.clone()),
                    MeshMaterial3d(cache.mat_hp_bg.clone()),
                    Transform::from_xyz(0.0, y_offset, 0.0).with_scale(Vec3::new(bar_w, 1.0, 1.0)),
                    Visibility::Hidden,
                    HealthBarRoot {
                        enemy: enemy_entity,
                        y_offset,
                        width: bar_w,
                    },
                ))
                .with_children(|bar| {
                    bar.spawn((
                        Mesh3d(cache.hp_fill_mesh.clone()),
                        MeshMaterial3d(cache.mat_hp_green.clone()),
                        Transform::from_xyz(0.0, 0.0, 0.01),
                        HealthBarFill {
                            enemy: enemy_entity,
                        },
                    ));
                });
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_enemy_archetype_mesh(
    body: &mut ChildSpawnerCommands<'_>,
    cache: &mut VisualCache,
    materials: &mut Assets<StandardMaterial>,
    enemy_entity: Entity,
    arch: EnemyArchetypeId,
    is_boss: bool,
    primary_mat: Handle<StandardMaterial>,
    accent_mat: Handle<StandardMaterial>,
) {
    match arch {
        EnemyArchetypeId::RatSwarm | EnemyArchetypeId::BossRatKing => {
            // Nimble body + pointed snout + ears + tail
            body.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(primary_mat.clone()),
                Transform::from_xyz(0.0, 0.20, 0.0).with_scale(Vec3::new(0.42, 0.28, 0.62)),
            ));
            body.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(primary_mat),
                Transform::from_xyz(0.0, 0.22, -0.44)
                    .with_scale(Vec3::new(0.24, 0.34, 0.24))
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            ));
            for sx in [-0.14, 0.14] {
                body.spawn((
                    Mesh3d(cache.unit_sphere.clone()),
                    MeshMaterial3d(accent_mat.clone()),
                    Transform::from_xyz(sx, 0.36, -0.22).with_scale(Vec3::splat(0.14)),
                ));
            }
            body.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(accent_mat.clone()),
                Transform::from_xyz(0.0, 0.18, 0.45)
                    .with_scale(Vec3::new(0.06, 0.42, 0.06))
                    .with_rotation(Quat::from_rotation_x(1.1)),
            ));
        }
        EnemyArchetypeId::GoblinRaider => {
            // Green torso + head + pointed ears + club
            body.spawn((
                Mesh3d(cache.capsule_mesh.clone()),
                MeshMaterial3d(primary_mat.clone()),
                Transform::from_xyz(0.0, 0.36, 0.0).with_scale(Vec3::new(0.85, 0.72, 0.85)),
            ));
            body.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(primary_mat.clone()),
                Transform::from_xyz(0.0, 0.72, 0.0).with_scale(Vec3::splat(0.36)),
            ));
            for sx in [-0.22, 0.22] {
                body.spawn((
                    Mesh3d(cache.unit_cone.clone()),
                    MeshMaterial3d(primary_mat.clone()),
                    Transform::from_xyz(sx, 0.75, 0.0)
                        .with_scale(Vec3::new(0.10, 0.24, 0.10))
                        .with_rotation(Quat::from_rotation_z(-sx.signum() * 1.1)),
                ));
            }
            body.spawn((
                Mesh3d(cache.unit_cylinder.clone()),
                MeshMaterial3d(cache.mat_wood.clone()),
                Transform::from_xyz(0.28, 0.42, -0.18)
                    .with_scale(Vec3::new(0.09, 0.42, 0.09))
                    .with_rotation(Quat::from_rotation_x(-0.7)),
            ));
        }
        EnemyArchetypeId::BatFlyer
        | EnemyArchetypeId::Gargoyle
        | EnemyArchetypeId::BossObsidianWing => {
            // Body + glowing eyes + articulated flapping wings
            body.spawn((
                Mesh3d(cache.capsule_mesh.clone()),
                MeshMaterial3d(primary_mat.clone()),
                Transform::from_xyz(0.0, 0.25, 0.0)
                    .with_scale(Vec3::new(0.70, 0.65, 0.90))
                    .with_rotation(Quat::from_rotation_x(0.45)),
            ));
            for sx in [-0.10, 0.10] {
                body.spawn((
                    Mesh3d(cache.unit_sphere.clone()),
                    MeshMaterial3d(accent_mat.clone()),
                    Transform::from_xyz(sx, 0.38, -0.26).with_scale(Vec3::splat(0.10)),
                ));
            }
            for side in [-1.0_f32, 1.0_f32] {
                body.spawn((
                    Mesh3d(cache.wing_mesh.clone()),
                    MeshMaterial3d(primary_mat.clone()),
                    Transform::from_xyz(side * 0.45, 0.30, 0.0),
                    EnemyWing {
                        enemy: enemy_entity,
                        side,
                        base_y: 0.30,
                    },
                ));
            }
        }
        EnemyArchetypeId::SlimeSplitter
        | EnemyArchetypeId::MiniSlime
        | EnemyArchetypeId::BossSlimeTitan => {
            // Glossy translucent emerald dome + inner core
            let slime_glass = cache.color_mat(
                materials,
                Color::srgba(0.24, 0.90, 0.42, 0.78),
                0.10,
                0.05,
                Some(LinearRgba::rgb(0.15, 0.65, 0.25)),
            );
            body.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(slime_glass),
                Transform::from_xyz(0.0, 0.34, 0.0).with_scale(Vec3::new(0.88, 0.68, 0.88)),
            ));
            body.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(accent_mat.clone()),
                Transform::from_xyz(0.0, 0.30, 0.0).with_scale(Vec3::splat(0.36)),
            ));
        }
        EnemyArchetypeId::VampireLord
        | EnemyArchetypeId::BossBloodCount
        | EnemyArchetypeId::BossAetherDreadlord => {
            // Tall dark cloaked figure + crimson collar + glowing eyes
            body.spawn((
                Mesh3d(cache.unit_cone.clone()),
                MeshMaterial3d(primary_mat),
                Transform::from_xyz(0.0, 0.48, 0.0).with_scale(Vec3::new(0.68, 0.96, 0.68)),
            ));
            body.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(cache.mat_bone.clone()),
                Transform::from_xyz(0.0, 0.92, 0.0).with_scale(Vec3::splat(0.28)),
            ));
            body.spawn((
                Mesh3d(cache.unit_torus.clone()),
                MeshMaterial3d(accent_mat.clone()),
                Transform::from_xyz(0.0, 0.78, 0.0).with_scale(Vec3::new(0.36, 0.4, 0.36)),
            ));
            for sx in [-0.06, 0.06] {
                body.spawn((
                    Mesh3d(cache.unit_sphere.clone()),
                    MeshMaterial3d(accent_mat.clone()),
                    Transform::from_xyz(sx, 0.94, -0.12).with_scale(Vec3::splat(0.06)),
                ));
            }
        }
        EnemyArchetypeId::StoneGolem | EnemyArchetypeId::BossGolemColossus => {
            // Bulky stone torso + heavy shoulders + glowing amber core rune
            body.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(primary_mat.clone()),
                Transform::from_xyz(0.0, 0.48, 0.0).with_scale(Vec3::new(0.62, 0.62, 0.44)),
            ));
            for sx in [-0.42, 0.42] {
                body.spawn((
                    Mesh3d(cache.unit_cube.clone()),
                    MeshMaterial3d(primary_mat.clone()),
                    Transform::from_xyz(sx, 0.56, 0.0).with_scale(Vec3::new(0.28, 0.46, 0.34)),
                ));
            }
            body.spawn((
                Mesh3d(cache.unit_cube.clone()),
                MeshMaterial3d(primary_mat),
                Transform::from_xyz(0.0, 0.90, 0.0).with_scale(Vec3::new(0.32, 0.26, 0.30)),
            ));
            body.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(accent_mat.clone()),
                Transform::from_xyz(0.0, 0.50, -0.22).with_scale(Vec3::splat(0.24)),
            ));
        }
    }

    // Boss crown & glowing ground aura ring
    if is_boss {
        body.spawn((
            Mesh3d(cache.unit_torus.clone()),
            MeshMaterial3d(cache.mat_gold.clone()),
            Transform::from_xyz(0.0, 1.08, 0.0).with_scale(Vec3::new(0.38, 0.45, 0.38)),
        ));
        body.spawn((
            Mesh3d(cache.thin_torus.clone()),
            MeshMaterial3d(accent_mat),
            Transform::from_xyz(0.0, 0.06, 0.0).with_scale(Vec3::new(1.18, 0.35, 1.18)),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn animate_enemies_and_status(
    time: Res<Time>,
    enemies: Query<(&Enemy, &StatusEffects, &GlobalTransform), Without<PendingDespawn>>,
    mut bodies: Query<(&EnemyAnimBody, &mut Transform), Without<EnemyWing>>,
    mut wings: Query<(&EnemyWing, &mut Transform), Without<EnemyAnimBody>>,
    mut freeze_shells: Query<(&EnemyFreezeShell, &mut Visibility), (Without<EnemyEnrageTint>, Without<EnemyLoopRing>)>,
    mut enrage_tints: Query<(&EnemyEnrageTint, &mut Visibility), (Without<EnemyFreezeShell>, Without<EnemyLoopRing>)>,
    mut loop_rings: Query<(&EnemyLoopRing, &mut Visibility), (Without<EnemyFreezeShell>, Without<EnemyEnrageTint>)>,
    mut vfx_writer: MessageWriter<SpawnVfxMsg>,
) {
    let elapsed = time.elapsed_secs();

    for (body, mut tf) in &mut bodies {
        let Ok((enemy, status, _)) = enemies.get(body.enemy) else {
            continue;
        };
        if status.is_stunned(enemy.is_boss) {
            continue;
        }
        let phase = enemy.anim_phase + elapsed * 7.0;
        let s = body.scale;
        match body.kind {
            EnemyAnimKind::Hop => {
                tf.translation.y = body.base_y + phase.sin().abs() * 0.14 * s;
                tf.rotation = Quat::from_rotation_z((phase * 0.5).sin() * 0.08);
            }
            EnemyAnimKind::FloatSway => {
                tf.translation.y = body.base_y + 0.12 + (phase * 0.45).sin() * 0.09;
                tf.rotation = Quat::from_rotation_z((phase * 0.35).sin() * 0.06);
            }
            EnemyAnimKind::SquashStretch => {
                let wobble = (phase * 0.7).sin() * 0.14;
                tf.scale = Vec3::new(s * (1.0 - wobble * 0.5), s * (1.0 + wobble), s * (1.0 - wobble * 0.5));
            }
            EnemyAnimKind::HeavyStomp => {
                tf.translation.y = body.base_y + (phase * 0.45).sin().abs() * 0.06;
                tf.rotation = Quat::from_rotation_z((phase * 0.45).sin() * 0.06);
            }
            EnemyAnimKind::FlyBob => {
                tf.translation.y = body.base_y + (phase * 0.6).sin() * 0.10;
            }
        }
    }

    for (wing, mut tf) in &mut wings {
        let Ok((enemy, status, _)) = enemies.get(wing.enemy) else {
            continue;
        };
        let speed = if status.is_stunned(enemy.is_boss) { 1.0 } else { 11.0 };
        let flap = (enemy.anim_phase + elapsed * speed).sin() * 0.58;
        tf.rotation = Quat::from_rotation_z(wing.side * flap);
        tf.translation.y = wing.base_y + flap.abs() * 0.06;
    }

    for (shell, mut vis) in &mut freeze_shells {
        if let Ok((_, status, _)) = enemies.get(shell.enemy) {
            *vis = if status.is_frozen() {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }

    for (tint, mut vis) in &mut enrage_tints {
        if let Ok((_, status, _)) = enemies.get(tint.enemy) {
            *vis = if status.enrage_timer > 0.0 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }

    for (ring, mut vis) in &mut loop_rings {
        if let Ok((enemy, _, _)) = enemies.get(ring.enemy) {
            *vis = if enemy.loop_count > 0 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }

    // Occasional orange sparks on burning enemies (visual clarity, lightweight rate)
    if ((elapsed * 60.0) as u32).is_multiple_of(14) {
        for (enemy, status, gtf) in &enemies {
            if status.is_burning() {
                vfx_writer.write(SpawnVfxMsg {
                    kind: VfxKind::ImpactSpark,
                    pos: gtf.translation() + Vec3::new(0.0, 0.4 * enemy.scale, 0.0),
                    target_pos: None,
                    color: Color::srgb(1.0, 0.45, 0.10),
                    radius: 0.35,
                });
            }
        }
    }
}

fn update_health_bars(
    camera_q: Query<&GlobalTransform, With<MainCamera>>,
    enemies: Query<(&Enemy, &GlobalTransform), Without<PendingDespawn>>,
    selection: Res<SelectionState>,
    settings: Res<GameSettings>,
    mut roots: Query<(&HealthBarRoot, &mut Visibility, &mut Transform), Without<HealthBarFill>>,
    mut fills: Query<(&HealthBarFill, &mut Transform, &mut MeshMaterial3d<StandardMaterial>), Without<HealthBarRoot>>,
    cache: Res<VisualCache>,
) {
    let cam_rot = camera_q
        .iter()
        .next()
        .map(|gtf| gtf.compute_transform().rotation)
        .unwrap_or(Quat::IDENTITY);

    for (root, mut vis, mut tf) in &mut roots {
        let Ok((enemy, enemy_gtf)) = enemies.get(root.enemy) else {
            *vis = Visibility::Hidden;
            continue;
        };

        let should_show = enemy.hp < enemy.max_hp
            || enemy.is_boss
            || selection.show_all_hp_bars
            || settings.always_show_hp_bars;

        *vis = if should_show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };

        if should_show {
            let enemy_rot = enemy_gtf.compute_transform().rotation;
            tf.rotation = enemy_rot.inverse() * cam_rot;
            tf.translation = Vec3::new(0.0, root.y_offset, 0.0);
        }
    }

    for (fill, mut tf, mut mat) in &mut fills {
        let Ok((enemy, _)) = enemies.get(fill.enemy) else {
            continue;
        };
        let frac = (enemy.hp / enemy.max_hp.max(1.0)).clamp(0.0, 1.0);
        tf.scale.x = frac.max(0.001);
        tf.translation.x = -0.48 * (1.0 - frac);

        let target_mat = if frac > 0.55 {
            cache.mat_hp_green.clone()
        } else if frac > 0.25 {
            cache.mat_hp_yellow.clone()
        } else {
            cache.mat_hp_red.clone()
        };
        if mat.0 != target_mat {
            mat.0 = target_mat;
        }
    }
}

fn animate_map_accents_and_torches(
    time: Res<Time>,
    mut rings: Query<(&MapPortalRing, &mut Transform), Without<FlickeringTorchLight>>,
    mut torches: Query<(&FlickeringTorchLight, &mut PointLight), Without<MapPortalRing>>,
    grid: Res<GridMap>,
    mut commands: Commands,
    cache: Res<VisualCache>,
    existing_reclaim: Query<(Entity, &ReclaimOverlayTile)>,
) {
    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();

    for (ring, mut tf) in &mut rings {
        tf.rotate_y(ring.spin_speed * dt);
        tf.translation.y = ring.base_y + (elapsed * 2.5 + ring.pulse_phase).sin() * 0.025;
    }

    for (torch, mut light) in &mut torches {
        let flicker = 1.0
            + (elapsed * 13.0 + torch.phase).sin() * 0.09
            + (elapsed * 23.0 + torch.phase * 1.7).cos() * 0.06;
        light.intensity = torch.base_intensity * flicker;
    }

    // Subtle red reclaim lock tile overlay during active wave sells
    let mut active_reclaims = std::collections::HashMap::new();
    for z in 0..grid.height {
        for x in 0..grid.width {
            let c = TileCoord::new(x, z);
            if grid.reclaim_remaining(c) > 0.0 {
                active_reclaims.insert(c, grid.tile_to_world(c));
            }
        }
    }

    for (e, ov) in &existing_reclaim {
        if active_reclaims.remove(&ov.coord).is_none()
            && let Ok(mut ec) = commands.get_entity(e)
        {
            ec.despawn();
        }
    }
    for (coord, world) in active_reclaims {
        commands.spawn((
            Mesh3d(cache.flat_disc.clone()),
            MeshMaterial3d(cache.mat_reclaim_lock.clone()),
            Transform::from_xyz(world.x, 0.025, world.z).with_scale(Vec3::new(0.90, 1.0, 0.90)),
            Visibility::default(),
            ReclaimOverlayTile { coord },
            GameplayEntity,
        ));
    }
}

// ============================================================================
// Projectiles, Lightning Arcs, Ground Hazards & Pooled Particles
// ============================================================================

fn spawn_projectile_visuals(
    mut commands: Commands,
    projectiles: Query<(Entity, &Projectile), (Without<ProjectileVisualAdded>, Without<PendingDespawn>)>,
    mut cache: ResMut<VisualCache>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, proj) in &projectiles {
        let Ok(mut ec) = commands.get_entity(entity) else {
            continue;
        };
        let s = proj.color.to_srgba();
        let mat = cache.color_mat(
            &mut materials,
            proj.color,
            0.20,
            0.15,
            Some(LinearRgba::rgb(s.red * 2.2, s.green * 2.2, s.blue * 2.2)),
        );
        let (mesh, scale) = if proj.is_heavy {
            (cache.unit_sphere.clone(), Vec3::splat(0.32))
        } else if proj.splash_radius > 0.1 {
            (cache.unit_sphere.clone(), Vec3::splat(0.24))
        } else {
            (cache.bolt_mesh.clone(), Vec3::splat(0.85))
        };

        ec.insert((
            ProjectileVisualAdded,
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Visibility::default(),
        ))
        .with_children(|p| {
            // Small glowing core dot
            p.spawn((
                Mesh3d(cache.unit_sphere.clone()),
                MeshMaterial3d(cache.mat_gold.clone()),
                Transform::from_scale(scale * 0.45),
            ));
        });
    }
}

fn spawn_hazard_visuals(
    mut commands: Commands,
    hazards: Query<(Entity, &GroundHazard), (Without<HazardVisualAdded>, Without<PendingDespawn>)>,
    cache: Res<VisualCache>,
) {
    for (entity, hazard) in &hazards {
        let Ok(mut ec) = commands.get_entity(entity) else {
            continue;
        };
        let d = hazard.radius * 2.0;
        ec.insert((
            HazardVisualAdded,
            Mesh3d(cache.flat_disc.clone()),
            MeshMaterial3d(cache.mat_poison_pool.clone()),
            Transform::from_xyz(hazard.pos.x, 0.03, hazard.pos.z).with_scale(Vec3::new(d, 1.0, d)),
            Visibility::default(),
        ));
    }
}

fn handle_vfx_messages(
    mut commands: Commands,
    mut reader: MessageReader<SpawnVfxMsg>,
    active_particles: Query<Entity, (With<ParticleVfx>, Without<PendingDespawn>)>,
    mut cache: ResMut<VisualCache>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut current_count = active_particles.iter().count();

    for msg in reader.read() {
        if matches!(msg.kind, VfxKind::LightningArc) {
            let end = msg.target_pos.unwrap_or(msg.pos + Vec3::Y);
            commands.spawn((
                LightningArcVfx {
                    start: msg.pos,
                    end,
                    color: msg.color,
                    timer: 0.18,
                    max_timer: 0.18,
                },
                GameplayEntity,
            ));
            continue;
        }

        if current_count >= MAX_ACTIVE_PARTICLES {
            continue;
        }

        let burst_count = match msg.kind {
            VfxKind::FireExplosion | VfxKind::UpgradeBurst | VfxKind::EnemyDeath => 10,
            VfxKind::IceShatter | VfxKind::PortalWarp => 8,
            VfxKind::PoisonCloud | VfxKind::SteamCloud | VfxKind::BuildDust => 6,
            VfxKind::ImpactSpark => 3,
            VfxKind::LightningArc => 0,
        };

        let s = msg.color.to_srgba();
        let mat = cache.color_mat(
            &mut materials,
            msg.color,
            0.25,
            0.1,
            Some(LinearRgba::rgb(s.red * 2.4, s.green * 2.4, s.blue * 2.4)),
        );

        let allowed = burst_count.min(MAX_ACTIVE_PARTICLES - current_count);
        for i in 0..allowed {
            let angle = (i as f32 / allowed.max(1) as f32) * std::f32::consts::TAU
                + rand::random::<f32>() * 0.5;
            let speed = (1.4 + rand::random::<f32>() * 2.6) * msg.radius.clamp(0.4, 2.5);
            let vy = match msg.kind {
                VfxKind::PoisonCloud | VfxKind::SteamCloud | VfxKind::BuildDust => {
                    0.9 + rand::random::<f32>() * 1.1
                }
                _ => 2.2 + rand::random::<f32>() * 2.5,
            };
            let gravity = match msg.kind {
                VfxKind::PoisonCloud | VfxKind::SteamCloud => -1.5,
                _ => 8.5,
            };
            let max_timer = match msg.kind {
                VfxKind::ImpactSpark => 0.25,
                _ => 0.45 + rand::random::<f32>() * 0.2,
            };
            let start_scale = match msg.kind {
                VfxKind::ImpactSpark => 0.08,
                VfxKind::PoisonCloud | VfxKind::SteamCloud => 0.18,
                _ => 0.13,
            };
            let mesh = if i % 2 == 0 {
                cache.unit_cube.clone()
            } else {
                cache.unit_sphere.clone()
            };

            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(msg.pos).with_scale(Vec3::splat(start_scale)),
                Visibility::default(),
                ParticleVfx {
                    velocity: Vec3::new(angle.cos() * speed, vy, angle.sin() * speed),
                    gravity,
                    timer: max_timer,
                    max_timer,
                    start_scale,
                },
                GameplayEntity,
            ));
            current_count += 1;
        }
    }
}

fn render_lightning_arcs_and_status_rings(
    mut commands: Commands,
    time: Res<Time>,
    mut arcs: Query<(Entity, &mut LightningArcVfx), Without<PendingDespawn>>,
    enemies: Query<(&Enemy, &StatusEffects, &GlobalTransform), Without<PendingDespawn>>,
    mut gizmos: Gizmos,
) {
    let dt = time.delta_secs();

    for (entity, mut arc) in &mut arcs {
        arc.timer -= dt;
        if arc.timer <= 0.0 {
            if let Ok(mut ec) = commands.get_entity(entity) {
                ec.insert(PendingDespawn);
            }
            continue;
        }
        let alpha = (arc.timer / arc.max_timer.max(0.01)).clamp(0.0, 1.0);
        let col = arc.color.with_alpha(alpha);
        let segments = 5;
        let mut prev = arc.start;
        for i in 1..=segments {
            let t = i as f32 / segments as f32;
            let mut pt = arc.start.lerp(arc.end, t);
            if i < segments {
                let jitter = (arc.start.distance(arc.end) * 0.12).clamp(0.08, 0.35);
                pt += Vec3::new(
                    (rand::random::<f32>() - 0.5) * jitter,
                    (rand::random::<f32>() - 0.5) * jitter,
                    (rand::random::<f32>() - 0.5) * jitter,
                );
            }
            gizmos.line(prev, pt, col);
            prev = pt;
        }
    }

    // Subtle purple armor-shred ring beneath affected enemies
    for (enemy, status, gtf) in &enemies {
        if status.armor_shred > 0.0 {
            let mut pos = gtf.translation();
            pos.y = if enemy.domain == MovementDomain::Air {
                AIR_ALTITUDE - 0.15
            } else {
                0.05
            };
            let iso = Isometry3d::new(pos, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
            gizmos.circle(
                iso,
                0.42 * enemy.scale,
                Color::srgba(0.68, 0.22, 0.95, 0.80),
            );
        }
    }
}

fn update_particles_and_hazards(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut ParticleVfx, &mut Transform), Without<PendingDespawn>>,
    mut hazards: Query<(Entity, &mut GroundHazard, &mut Transform), (Without<ParticleVfx>, Without<PendingDespawn>)>,
) {
    let dt = time.delta_secs();

    for (entity, mut p, mut tf) in &mut particles {
        p.timer -= dt;
        if p.timer <= 0.0 {
            if let Ok(mut ec) = commands.get_entity(entity) {
                ec.insert(PendingDespawn);
            }
            continue;
        }
        p.velocity.y -= p.gravity * dt;
        tf.translation += p.velocity * dt;
        if tf.translation.y < 0.04 {
            tf.translation.y = 0.04;
            p.velocity.y = p.velocity.y.abs() * 0.3;
        }
        let frac = (p.timer / p.max_timer.max(0.01)).clamp(0.0, 1.0);
        tf.scale = Vec3::splat((p.start_scale * frac).max(0.01));
    }

    for (entity, mut h, mut tf) in &mut hazards {
        h.timer -= dt;
        if h.timer <= 0.0 {
            if let Ok(mut ec) = commands.get_entity(entity) {
                ec.insert(PendingDespawn);
            }
            continue;
        }
        let pulse = 1.0 + (time.elapsed_secs() * 5.0).sin() * 0.06;
        let d = h.radius * 2.0 * pulse * (h.timer.clamp(0.0, 0.6) / 0.6).max(0.4);
        tf.scale = Vec3::new(d, 1.0, d);
    }
}
