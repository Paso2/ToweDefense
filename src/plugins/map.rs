use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::{AccumulatedMouseMotion, MouseButton, MouseWheel};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::types::{
    AppState, BuildTowerRequest, CameraRig, Enemy, FlowField, GameConfigs, GameSet, GameSettings,
    GameSpeed, GameplayEntity, GridMap, MainCamera, MapConfig, MatchConfig, SelectionState,
    SellTowerRequest, SendWaveEarlyRequest, StartMatchRequest, TileKind, Tower,
    UpgradeTowerRequest,
};

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_main_camera).add_systems(
            Update,
            (
                handle_start_match_requests,
                camera_controller_system.run_if(in_state(AppState::Playing)),
                battlefield_input_system.run_if(in_state(AppState::Playing)),
                handle_pause_escape_in_paused.run_if(in_state(AppState::Paused)),
            )
                .chain()
                .in_set(GameSet::InputSet),
        );
    }
}

fn spawn_main_camera(mut commands: Commands, rig: Res<CameraRig>) {
    let cam_transform = compute_camera_transform(rig.focus, rig.distance, rig.pitch_rad, rig.yaw_rad);
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 42.0_f32.to_radians(),
            ..default()
        }),
        cam_transform,
        Tonemapping::TonyMcMapface,
        Bloom {
            intensity: 0.15,
            ..default()
        },
        MainCamera,
    ));
}

#[inline]
fn compute_camera_transform(focus: Vec3, distance: f32, pitch_rad: f32, yaw_rad: f32) -> Transform {
    let horizontal = distance * pitch_rad.cos();
    let vertical = distance * pitch_rad.sin();
    let offset = Vec3::new(
        horizontal * yaw_rad.sin(),
        vertical,
        horizontal * yaw_rad.cos(),
    );
    Transform::from_translation(focus + offset).looking_at(focus, Vec3::Y)
}

fn populate_grid_from_config(grid: &mut GridMap, cfg: &MapConfig) {
    let total = (cfg.width.max(0) * cfg.height.max(0)) as usize;
    grid.map_id = cfg.id;
    grid.biome = cfg.biome;
    grid.width = cfg.width;
    grid.height = cfg.height;
    grid.tiles = vec![TileKind::Buildable; total];
    grid.reclaim_timers = vec![0.0; total];
    grid.spawns = cfg.spawns.iter().map(|z| z.tiles()).collect();
    grid.checkpoints = cfg.checkpoints.iter().map(|z| z.tiles()).collect();
    grid.goals = cfg.goal.tiles();
    grid.version = grid.version.wrapping_add(1);

    for &coord in &cfg.blocked_tiles {
        if let Some(idx) = grid.idx(coord) {
            grid.tiles[idx] = TileKind::BlockedTerrain;
        }
    }

    for (idx, zone) in grid.spawns.iter().enumerate() {
        let kind = TileKind::Spawn(idx as u8);
        for &coord in zone {
            if let Some(cell) = grid.idx(coord) {
                grid.tiles[cell] = kind;
            }
        }
    }

    for (idx, zone) in grid.checkpoints.iter().enumerate() {
        let kind = TileKind::Checkpoint(idx as u8);
        for &coord in zone {
            if let Some(cell) = grid.idx(coord) {
                grid.tiles[cell] = kind;
            }
        }
    }

    for &coord in &grid.goals {
        if let Some(cell) = grid.idx(coord) {
            grid.tiles[cell] = TileKind::Goal;
        }
    }

    grid.dirty = true;
}

#[allow(clippy::too_many_arguments)]
fn handle_start_match_requests(
    mut commands: Commands,
    mut requests: MessageReader<StartMatchRequest>,
    configs: Res<GameConfigs>,
    mut match_cfg: ResMut<MatchConfig>,
    mut grid: ResMut<GridMap>,
    mut flow: ResMut<FlowField>,
    mut rig: ResMut<CameraRig>,
    mut selection: ResMut<SelectionState>,
    mut speed: ResMut<GameSpeed>,
    gameplay_entities: Query<Entity, With<GameplayEntity>>,
    mut camera_q: Query<&mut Transform, With<MainCamera>>,
) {
    let Some(req) = requests.read().last().cloned() else {
        return;
    };

    for entity in &gameplay_entities {
        if let Ok(mut ec) = commands.get_entity(entity) {
            ec.despawn();
        }
    }

    let map_cfg = configs
        .get_map(req.map_id)
        .or_else(|| configs.maps.values().next())
        .cloned()
        .expect("at least one MapConfig must exist");

    match_cfg.map_id = map_cfg.id;
    match_cfg.faction_id = req.faction_id;
    match_cfg.difficulty = req.difficulty;
    match_cfg.endless_active = false;

    populate_grid_from_config(&mut grid, &map_cfg);

    *flow = FlowField {
        width: grid.width,
        height: grid.height,
        ..default()
    };

    let half_w = grid.width as f32 * 0.5;
    let half_h = grid.height as f32 * 0.5;
    *rig = CameraRig {
        focus: Vec3::ZERO,
        target_focus: Vec3::ZERO,
        distance: 26.0,
        target_distance: 26.0,
        pitch_rad: 52.0_f32.to_radians(),
        yaw_rad: 0.0,
        target_yaw: 0.0,
        bounds_min: Vec2::new(-half_w + 1.0, -half_h + 1.0),
        bounds_max: Vec2::new(half_w - 1.0, half_h - 1.0),
    };

    if let Ok(mut cam_transform) = camera_q.single_mut() {
        *cam_transform =
            compute_camera_transform(rig.focus, rig.distance, rig.pitch_rad, rig.yaw_rad);
    }

    *selection = SelectionState::default();
    *speed = GameSpeed::Normal;
}

#[allow(clippy::too_many_arguments)]
fn camera_controller_system(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut wheel_reader: MessageReader<MouseWheel>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    settings: Res<GameSettings>,
    selection: Res<SelectionState>,
    mut rig: ResMut<CameraRig>,
    mut camera_q: Query<&mut Transform, With<MainCamera>>,
) {
    let dt = time.delta_secs().min(0.05);

    // Center reset on Home or C
    if keys.just_pressed(KeyCode::Home) || keys.just_pressed(KeyCode::KeyC) {
        rig.target_focus = Vec3::ZERO;
        rig.target_distance = 26.0;
        rig.target_yaw = 0.0;
    }

    // Panning direction in camera-aligned XZ plane
    let mut pan_local = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        pan_local.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        pan_local.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        pan_local.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        pan_local.x += 1.0;
    }

    // Note: Q/W/E/A/S/D also serve as command-card hotkeys on just_pressed, while WASD & Arrow keys
    // drive smooth camera panning when held.
    if settings.edge_scrolling
        && !selection.ui_pointer_captured
        && let Ok(window) = window_q.single()
        && let Some(cursor) = window.cursor_position()
    {
        let margin = 16.0;
        let w = window.width();
        let h = window.height();
        if cursor.x <= margin {
            pan_local.x -= 1.0;
        } else if cursor.x >= w - margin {
            pan_local.x += 1.0;
        }
        if cursor.y <= margin {
            pan_local.y -= 1.0;
        } else if cursor.y >= h - margin {
            pan_local.y += 1.0;
        }
    }

    let sin_y = rig.yaw_rad.sin();
    let cos_y = rig.yaw_rad.cos();
    let right_xz = Vec3::new(cos_y, 0.0, -sin_y);
    let back_xz = Vec3::new(sin_y, 0.0, cos_y);

    if pan_local.length_squared() > 0.0 {
        let dir = pan_local.normalize();
        let pan_speed = settings.camera_pan_speed.max(6.0);
        let world_delta = (right_xz * dir.x + back_xz * dir.y) * pan_speed * dt;
        rig.target_focus += world_delta;
    }

    // Middle-mouse drag pans; Alt+Middle or Right-drag orbits yaw smoothly (clamped ±35 deg).
    let alt_held = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let delta = mouse_motion.delta;
    if delta.length_squared() > 0.0 && !selection.ui_pointer_captured {
        if mouse_buttons.pressed(MouseButton::Middle) && !alt_held {
            let drag_scale = (rig.distance / 26.0) * 0.032;
            rig.target_focus -= right_xz * delta.x * drag_scale;
            rig.target_focus -= back_xz * delta.y * drag_scale;
        } else if (mouse_buttons.pressed(MouseButton::Middle) && alt_held)
            || mouse_buttons.pressed(MouseButton::Right)
        {
            let max_yaw = 35.0_f32.to_radians();
            rig.target_yaw = (rig.target_yaw - delta.x * 0.005).clamp(-max_yaw, max_yaw);
        }
    }

    // Mouse wheel zoom
    if !selection.ui_pointer_captured {
        for wheel in wheel_reader.read() {
            let scroll = match wheel.unit {
                bevy::input::mouse::MouseScrollUnit::Line => wheel.y,
                bevy::input::mouse::MouseScrollUnit::Pixel => wheel.y * 0.02,
            };
            rig.target_distance = (rig.target_distance - scroll * 2.2).clamp(12.0, 36.0);
        }
    } else {
        wheel_reader.clear();
    }

    // Clamp target focus within map bounds
    rig.target_focus.x = rig.target_focus.x.clamp(rig.bounds_min.x, rig.bounds_max.x);
    rig.target_focus.z = rig.target_focus.z.clamp(rig.bounds_min.y, rig.bounds_max.y);
    rig.target_focus.y = 0.0;

    // Smooth interpolation
    let lerp_factor = 1.0 - (-12.0 * dt).exp();
    rig.focus = rig.focus.lerp(rig.target_focus, lerp_factor);
    rig.distance += (rig.target_distance - rig.distance) * lerp_factor;
    rig.yaw_rad += (rig.target_yaw - rig.yaw_rad) * lerp_factor;

    // Couple pitch gently from 44 deg (distance 12) to 60 deg (distance 36)
    let zoom_t = ((rig.distance - 12.0) / (36.0 - 12.0)).clamp(0.0, 1.0);
    let target_pitch = (44.0_f32 + zoom_t * 16.0_f32).to_radians();
    rig.pitch_rad += (target_pitch - rig.pitch_rad) * lerp_factor;

    if let Ok(mut cam_transform) = camera_q.single_mut() {
        *cam_transform =
            compute_camera_transform(rig.focus, rig.distance, rig.pitch_rad, rig.yaw_rad);
    }
}

#[derive(SystemParam)]
struct BattlefieldActionWriters<'w> {
    build: MessageWriter<'w, BuildTowerRequest>,
    upgrade: MessageWriter<'w, UpgradeTowerRequest>,
    sell: MessageWriter<'w, SellTowerRequest>,
    send_wave: MessageWriter<'w, SendWaveEarlyRequest>,
}

#[derive(SystemParam)]
struct BattlefieldInputState<'w> {
    time: Res<'w, Time>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse_buttons: Res<'w, ButtonInput<MouseButton>>,
    mouse_motion: Res<'w, AccumulatedMouseMotion>,
    grid: Res<'w, GridMap>,
    configs: Res<'w, GameConfigs>,
    match_cfg: Res<'w, MatchConfig>,
}

#[allow(clippy::too_many_arguments)]
fn battlefield_input_system(
    input: BattlefieldInputState,
    mut right_dragged: Local<bool>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut flow: ResMut<FlowField>,
    mut selection: ResMut<SelectionState>,
    mut speed: ResMut<GameSpeed>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut towers: Query<(Entity, &mut Tower)>,
    enemies: Query<(Entity, &Transform, &Enemy)>,
    mut writers: BattlefieldActionWriters,
) {
    let keys = &input.keys;
    let mouse_buttons = &input.mouse_buttons;
    let grid = &input.grid;
    let configs = &input.configs;
    let match_cfg = &input.match_cfg;

    // Track right-drag so orbiting the camera with right-drag doesn't cancel selection on release.
    if mouse_buttons.just_pressed(MouseButton::Right) {
        *right_dragged = false;
    } else if mouse_buttons.pressed(MouseButton::Right)
        && input.mouse_motion.delta.length_squared() > 4.0
    {
        *right_dragged = true;
    }

    // Global hotkeys
    if keys.just_pressed(KeyCode::KeyP) {
        flow.show_path_overlay = !flow.show_path_overlay;
    }

    let alt_held = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    selection.show_all_hp_bars = alt_held;

    if keys.just_pressed(KeyCode::Space) {
        *speed = match *speed {
            GameSpeed::Paused => GameSpeed::Normal,
            _ => GameSpeed::Paused,
        };
    }
    if keys.just_pressed(KeyCode::Digit1) {
        *speed = GameSpeed::Normal;
    } else if keys.just_pressed(KeyCode::Digit2) {
        *speed = GameSpeed::Fast;
    } else if keys.just_pressed(KeyCode::Digit3) {
        *speed = GameSpeed::Ultra;
    }

    if keys.just_pressed(KeyCode::KeyN) {
        writers.send_wave.write(SendWaveEarlyRequest);
    }

    if keys.just_pressed(KeyCode::KeyT) && !selection.selected_towers.is_empty() {
        for &tower_entity in &selection.selected_towers {
            if let Ok((_, mut tower)) = towers.get_mut(tower_entity) {
                tower.priority = tower.priority.next();
            }
        }
    }

    // Raycast cursor onto y = 0 ground plane when UI is not capturing the pointer.
    let mut hit_world: Option<Vec3> = None;
    if !selection.ui_pointer_captured
        && let Ok(window) = window_q.single()
        && let Some(cursor_pos) = window.cursor_position()
        && let Ok((camera, cam_global)) = camera_q.single()
        && let Ok(ray) = camera.viewport_to_world(cam_global, cursor_pos)
        && let Some(point) = ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
    {
        hit_world = Some(point);
        selection.hovered_tile = grid.world_to_tile(point);
    } else if selection.ui_pointer_captured {
        selection.hovered_tile = None;
    }

    // Left-click: build or select
    if !selection.ui_pointer_captured && mouse_buttons.just_pressed(MouseButton::Left) {
        let shift_held = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        if let Some(tower_id) = selection.build_tower_id.clone() {
            if let Some(coord) = selection.hovered_tile {
                writers.build.write(BuildTowerRequest {
                    coord,
                    tower_id,
                    faction: match_cfg.faction_id,
                    free: false,
                });
                if !shift_held {
                    selection.build_tower_id = None;
                }
            }
        } else {
            let now = input.time.elapsed_secs();
            let clicked_tower = selection.hovered_tile.and_then(|coord| {
                if let Some(TileKind::Tower(entity)) = grid.get(coord) {
                    Some(entity)
                } else {
                    towers
                        .iter()
                        .find(|(_, t)| t.coord == coord)
                        .map(|(e, _)| e)
                }
            });

            if let Some(tower_entity) = clicked_tower {
                let is_double_click = selection.last_clicked_tower == Some(tower_entity)
                    && (now - selection.last_click_time) <= 0.35;

                if is_double_click {
                    if let Ok((_, clicked)) = towers.get(tower_entity) {
                        let clicked_id = clicked.id.clone();
                        selection.selected_towers = towers
                            .iter()
                            .filter(|(_, t)| t.id == clicked_id)
                            .map(|(e, _)| e)
                            .collect();
                    }
                } else {
                    selection.selected_towers = vec![tower_entity];
                }
                selection.selected_enemy = None;
                selection.last_clicked_tower = Some(tower_entity);
                selection.last_click_time = now;
            } else if let Some(world_pt) = hit_world {
                let mut best_enemy: Option<(Entity, f32)> = None;
                for (entity, transform, enemy) in &enemies {
                    if enemy.hp <= 0.0 {
                        continue;
                    }
                    let dist = Vec2::new(
                        transform.translation.x - world_pt.x,
                        transform.translation.z - world_pt.z,
                    )
                    .length();
                    if dist <= 0.85 && best_enemy.is_none_or(|(_, bd)| dist < bd) {
                        best_enemy = Some((entity, dist));
                    }
                }
                if let Some((enemy_entity, _)) = best_enemy {
                    selection.selected_towers.clear();
                    selection.selected_enemy = Some(enemy_entity);
                } else {
                    selection.selected_towers.clear();
                    selection.selected_enemy = None;
                }
                selection.last_clicked_tower = None;
                selection.last_click_time = now;
            } else {
                selection.selected_towers.clear();
                selection.selected_enemy = None;
                selection.last_clicked_tower = None;
            }
        }
    }

    // Right-click (without drag) or Escape: cancel build -> clear selection -> pause
    let right_clicked = !selection.ui_pointer_captured
        && mouse_buttons.just_released(MouseButton::Right)
        && !*right_dragged;
    let esc_pressed = keys.just_pressed(KeyCode::Escape);
    if right_clicked || esc_pressed {
        if selection.build_tower_id.is_some() {
            selection.build_tower_id = None;
        } else if !selection.selected_towers.is_empty() || selection.selected_enemy.is_some() {
            selection.selected_towers.clear();
            selection.selected_enemy = None;
        } else if esc_pressed {
            next_app_state.set(AppState::Paused);
        }
    }

    // Sell selected towers with X
    if keys.just_pressed(KeyCode::KeyX) && !selection.selected_towers.is_empty() {
        for &tower_entity in &selection.selected_towers {
            writers.sell.write(SellTowerRequest { tower_entity });
        }
        selection.selected_towers.clear();
    }

    // Command-card hotkeys: Q / W / E / A / S / D
    let pressed_hotkey = if keys.just_pressed(KeyCode::KeyQ) {
        Some("Q")
    } else if keys.just_pressed(KeyCode::KeyW) {
        Some("W")
    } else if keys.just_pressed(KeyCode::KeyE) {
        Some("E")
    } else if keys.just_pressed(KeyCode::KeyA) {
        Some("A")
    } else if keys.just_pressed(KeyCode::KeyS) {
        Some("S")
    } else if keys.just_pressed(KeyCode::KeyD) {
        Some("D")
    } else {
        None
    };

    if let Some(hotkey) = pressed_hotkey
        && let Some(faction_cfg) = configs.get_faction(match_cfg.faction_id)
    {
        if selection.selected_towers.is_empty() {
            // Root build selection
            if let Some(root_id) = faction_cfg.root_buildable_towers.iter().find(|id| {
                faction_cfg
                    .towers
                    .get(id.as_str())
                    .is_some_and(|cfg| cfg.hotkey.eq_ignore_ascii_case(hotkey))
            }) {
                selection.build_tower_id = Some(root_id.clone());
            }
        } else {
            // Upgrade selected towers matching this upgrade hotkey
            for &tower_entity in &selection.selected_towers {
                if let Ok((_, tower)) = towers.get(tower_entity)
                    && let Some(tower_cfg) = configs.get_tower(tower.faction, &tower.id)
                    && let Some(target_id) = tower_cfg.upgrades_to.iter().find(|up_id| {
                        configs
                            .get_tower(tower.faction, up_id.as_str())
                            .is_some_and(|up_cfg| up_cfg.hotkey.eq_ignore_ascii_case(hotkey))
                    })
                {
                    writers.upgrade.write(UpgradeTowerRequest {
                        tower_entity,
                        target_tower_id: target_id.clone(),
                        free: false,
                    });
                }
            }
        }
    }
}

fn handle_pause_escape_in_paused(
    keys: Res<ButtonInput<KeyCode>>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next_app_state.set(AppState::Playing);
    }
}
