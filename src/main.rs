#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use aethermaul_td::plugins::{
    AudioPlugin, CombatPlugin, EconomyPlugin, MapPlugin, PathfindingPlugin, PersistencePlugin,
    TowerPlugin, UiPlugin, VisualsPlugin, WavePlugin,
};
use aethermaul_td::types::*;
use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Capturing, Screenshot, save_to_disk},
    window::{Window, WindowPlugin, WindowResolution},
};

fn main() {
    let smoke_test = std::env::args().any(|arg| arg == "--smoke-test");

    let mut app = App::new();

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Aethermaul TD — 3D Fantasy Mazing Tower Defense".to_string(),
            resolution: WindowResolution::new(1280, 720),
            resizable: true,
            ..default()
        }),
        ..default()
    }));

    #[cfg(feature = "dev_tools")]
    {
        app.add_plugins(bevy_egui::EguiPlugin::default());
        app.add_plugins(bevy_inspector_egui::quick::WorldInspectorPlugin::new());
    }

    // Core states & shared resources
    app.init_state::<AppState>()
        .init_resource::<GameConfigs>()
        .init_resource::<MatchConfig>()
        .init_resource::<GridMap>()
        .init_resource::<FlowField>()
        .init_resource::<EconomyState>()
        .init_resource::<WaveState>()
        .init_resource::<SelectionState>()
        .init_resource::<CameraRig>()
        .init_resource::<GameSpeed>()
        .init_resource::<GameSettings>()
        .init_resource::<ProfileScores>()
        .insert_resource(SmokeTestState {
            enabled: smoke_test,
            ..default()
        });

    // Register all Bevy 0.20 buffered messages
    app.add_message::<StartMatchRequest>()
        .add_message::<BuildTowerRequest>()
        .add_message::<UpgradeTowerRequest>()
        .add_message::<SellTowerRequest>()
        .add_message::<SendWaveEarlyRequest>()
        .add_message::<DamageAppliedMsg>()
        .add_message::<EnemyKilledMsg>()
        .add_message::<EnemyLeakedMsg>()
        .add_message::<FreezeTriggeredMsg>()
        .add_message::<TowerBuiltMsg>()
        .add_message::<TowerUpgradedMsg>()
        .add_message::<TowerSoldMsg>()
        .add_message::<WaveStartedMsg>()
        .add_message::<WaveCompletedMsg>()
        .add_message::<PlaySfxMsg>()
        .add_message::<SpawnVfxMsg>()
        .add_message::<SpawnPopupMsg>()
        .add_message::<ConfigReloadMsg>()
        .add_message::<SavePersistenceMsg>();

    // Strict 10-stage SystemSet pipeline
    app.configure_sets(
        Update,
        (
            GameSet::InputSet,
            GameSet::BuildSet,
            GameSet::PathfindingSet,
            GameSet::SpawnSet,
            GameSet::MovementSet,
            GameSet::TargetingSet,
            GameSet::CombatSet,
            GameSet::LeakEconomySet,
            GameSet::CleanupSet,
            GameSet::VfxUiSet,
        )
            .chain(),
    );

    // Register all modular game plugins
    app.add_plugins((
        PersistencePlugin,
        MapPlugin,
        PathfindingPlugin,
        EconomyPlugin,
        TowerPlugin,
        CombatPlugin,
        WavePlugin,
        VisualsPlugin,
        AudioPlugin,
        UiPlugin,
    ));

    // Central safe despawn & smoke-test harness
    app.add_systems(Startup, start_smoke_test_if_enabled)
        .add_systems(
            Update,
            cleanup_pending_despawns.in_set(GameSet::CleanupSet),
        )
        .add_systems(
            Update,
            smoke_test_driver
                .in_set(GameSet::VfxUiSet)
                .run_if(|s: Res<SmokeTestState>| s.enabled),
        );

    app.run();
}

/// Safely despawns all entities marked `PendingDespawn` exclusively during `CleanupSet`.
fn cleanup_pending_despawns(
    mut commands: Commands,
    pending: Query<Entity, With<PendingDespawn>>,
    mut selection: ResMut<SelectionState>,
) {
    for entity in &pending {
        selection.selected_towers.retain(|&e| e != entity);
        if selection.selected_enemy == Some(entity) {
            selection.selected_enemy = None;
        }
        if selection.last_clicked_tower == Some(entity) {
            selection.last_clicked_tower = None;
        }
        if let Ok(mut entity_commands) = commands.get_entity(entity) {
            entity_commands.despawn();
        }
    }
}

fn start_smoke_test_if_enabled(
    smoke: Res<SmokeTestState>,
    mut start_writer: MessageWriter<StartMatchRequest>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if smoke.enabled {
        info!("--smoke-test mode enabled: launching automated verification sequence");
        start_writer.write(StartMatchRequest {
            map_id: MapId::LordaeronBastion,
            faction_id: FactionId::ElementalCircle,
            difficulty: Difficulty::Normal,
        });
        next_state.set(AppState::Playing);
    }
}

#[allow(clippy::too_many_arguments)]
fn smoke_test_driver(
    mut commands: Commands,
    time: Res<Time>,
    mut smoke: ResMut<SmokeTestState>,
    mut injected_showcase: Local<bool>,
    mut flow: ResMut<FlowField>,
    mut wave: ResMut<WaveState>,
    mut economy: ResMut<EconomyState>,
    mut selection: ResMut<SelectionState>,
    mut build_writer: MessageWriter<BuildTowerRequest>,
    mut send_wave_writer: MessageWriter<SendWaveEarlyRequest>,
    towers: Query<(Entity, &Tower)>,
    capturing: Query<Entity, With<Capturing>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    // Advance smoke clock in exact lockstep with clamped simulation dt (<= 0.05s)
    let step = time.delta_secs().min(0.05);
    smoke.elapsed += step;

    // Step 1 (t >= 0.15s): Build a genuine WC3 serpentine maze on Lordaeron-Bastion
    // with a bottom corridor wall at z=7 so enemies MUST weave UP -> DOWN -> UP -> DOWN -> UP!
    if !smoke.stage_01_maze_built && smoke.elapsed >= 0.15 {
        smoke.stage_01_maze_built = true;
        flow.show_path_overlay = true;
        wave.next_wave_has_air = true;

        let mut maze_coords: Vec<TileCoord> = Vec::new();
        // Horizontal lower corridor wall (z=7, x=1..=21) sealing the left gap under Spawn (1,1)
        // so enemies MUST snake inside z=0..=6 across all 4 vertical baffles!
        for x in 1..=21 {
            maze_coords.push(TileCoord::new(x, 7));
        }
        // Alternating vertical serpentine baffles inside the z=0..=6 corridor
        // Baffle 1: x=6, z=0..=4 (forces path DOWN around z=5..6)
        for z in 0..=4 {
            maze_coords.push(TileCoord::new(6, z));
        }
        // Baffle 2: x=10, z=2..=6 (forces path UP around z=0..1)
        for z in 2..=6 {
            maze_coords.push(TileCoord::new(10, z));
        }
        // Baffle 3: x=15, z=0..=4 (forces path DOWN around z=5..6)
        for z in 0..=4 {
            maze_coords.push(TileCoord::new(15, z));
        }
        // Baffle 4: x=19, z=2..=6 (forces path UP around z=0..1 toward CP0 at (24,2))
        for z in 2..=6 {
            maze_coords.push(TileCoord::new(19, z));
        }
        // Mid-lane serpentine baffles between CP0 -> CP1 -> CP2
        for z in 10..=13 {
            maze_coords.push(TileCoord::new(11, z));
        }
        for z in 11..=14 {
            maze_coords.push(TileCoord::new(17, z));
        }

        for coord in maze_coords {
            build_writer.write(BuildTowerRequest {
                coord,
                tower_id: "arcane_pillar".to_string(),
                faction: FactionId::ElementalCircle,
                free: true,
            });
        }
        info!("SmokeTest: requested true WC3 serpentine maze construction");
    }

    // Screenshot 1 (t >= 2.0s): Serpentine maze + ground & air (y=2.6) path preview
    if !smoke.stage_01_shot_taken && smoke.elapsed >= 2.0 {
        smoke.stage_01_shot_taken = true;
        info!("SmokeTest: capturing verify_01_maze.png");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("verify_01_maze.png"));
    }

    // Step 2 (t >= 2.4s): Place multi-faction & dual-element showcase towers + start active combat
    if !smoke.stage_02_combat_built && smoke.elapsed >= 2.4 {
        smoke.stage_02_combat_built = true;
        flow.show_path_overlay = false;

        // Give economy funds so our primary showcased Fire Obelisk is paid cleanly (real sell refund)
        economy.gold += 80;
        economy.special_currency += 3;

        // Paid T2 Fire Obelisk right at the serpentine bend (has 3 upgrades: Inferno / Steam / Plasma!)
        build_writer.write(BuildTowerRequest {
            coord: TileCoord::new(4, 3),
            tower_id: "fire_obelisk".to_string(),
            faction: FactionId::ElementalCircle,
            free: false,
        });

        let showcase_towers: [(TileCoord, FactionId, &str); 10] = [
            // Elemental Circle dual elements + Water around the snake killbox
            (
                TileCoord::new(8, 3),
                FactionId::ElementalCircle,
                "steam_condenser",
            ),
            (
                TileCoord::new(8, 5),
                FactionId::ElementalCircle,
                "plasma_core",
            ),
            (
                TileCoord::new(12, 2),
                FactionId::ElementalCircle,
                "storm_pillar",
            ),
            (
                TileCoord::new(8, 1),
                FactionId::ElementalCircle,
                "water_fountain",
            ),
            // Citadel
            (
                TileCoord::new(4, 5),
                FactionId::Citadel,
                "gatling_ballista",
            ),
            (
                TileCoord::new(12, 5),
                FactionId::Citadel,
                "volley_bastion",
            ),
            // Dragon Clan
            (
                TileCoord::new(12, 3),
                FactionId::DragonClan,
                "elder_fire_dragon",
            ),
            // Frostguard
            (
                TileCoord::new(17, 4),
                FactionId::Frostguard,
                "blizzard_spire",
            ),
            // Necropolis
            (
                TileCoord::new(17, 6),
                FactionId::Necropolis,
                "plague_ziggurat",
            ),
            (
                TileCoord::new(17, 2),
                FactionId::Necropolis,
                "banshee_tower",
            ),
        ];

        for (coord, faction, tower_id) in showcase_towers {
            build_writer.write(BuildTowerRequest {
                coord,
                tower_id: tower_id.to_string(),
                faction,
                free: true,
            });
        }

        // Start Wave 1 immediately
        send_wave_writer.write(SendWaveEarlyRequest);
        info!("SmokeTest: built multi-faction & dual-element showcase and launched Wave 1");
    }

    // Once Wave 1 is actually active, append resilient Ground + Air enemies so verify_02_combat.png
    // captures dense multi-target combat, flying Bats (y=2.6), projectiles, lightning arcs & 3D HP bars!
    if smoke.stage_02_combat_built && !*injected_showcase && wave.phase == WavePhase::WaveActive {
        *injected_showcase = true;
        let showcase_spawns: [(EnemyArchetypeId, f32, f32); 12] = [
            (EnemyArchetypeId::StoneGolem, 560.0, 0.05),
            (EnemyArchetypeId::BatFlyer, 360.0, 0.25),
            (EnemyArchetypeId::SlimeSplitter, 400.0, 0.50),
            (EnemyArchetypeId::VampireLord, 520.0, 0.75),
            (EnemyArchetypeId::StoneGolem, 600.0, 1.00),
            (EnemyArchetypeId::BatFlyer, 380.0, 1.25),
            (EnemyArchetypeId::SlimeSplitter, 420.0, 1.50),
            (EnemyArchetypeId::GoblinRaider, 260.0, 1.75),
            (EnemyArchetypeId::StoneGolem, 620.0, 2.00),
            (EnemyArchetypeId::BatFlyer, 400.0, 2.25),
            (EnemyArchetypeId::VampireLord, 540.0, 2.50),
            (EnemyArchetypeId::SlimeSplitter, 440.0, 2.75),
        ];
        for (archetype, hp, delay) in showcase_spawns {
            wave.spawn_queue.push(PendingSpawn {
                archetype,
                delay_remaining: delay,
                spawn_index: 0,
                hp_override: Some(hp),
                bounty_override: Some(8),
                wave_number: 1,
            });
        }
        wave.wave_spawn_done = false;
    }

    // Screenshot 2 (t >= 6.0s): Active Ground + Air combat with projectiles, lightning & 3D HP bars
    if !smoke.stage_02_shot_taken && smoke.elapsed >= 6.0 {
        smoke.stage_02_shot_taken = true;
        info!("SmokeTest: capturing verify_02_combat.png");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("verify_02_combat.png"));
    }

    // Step 3 (t >= 6.6s): Select the Fire Obelisk (shows live damage/kills, non-zero sell refund,
    // AND populates the 3-column [Q][W][E] Command Card with Inferno / Steam [Dual] / Plasma [Dual]!)
    if !smoke.stage_03_ui_selected && smoke.elapsed >= 6.6 {
        smoke.stage_03_ui_selected = true;
        if let Some((entity, _)) = towers
            .iter()
            .find(|(_, t)| t.id == "fire_obelisk")
            .or_else(|| towers.iter().next())
        {
            selection.selected_towers = vec![entity];
            selection.selected_enemy = None;
            info!("SmokeTest: selected Fire Obelisk for WC3 Inspector + 3-column Command Card UI");
        }
    }

    // Screenshot 3 (t >= 8.0s): WC3 HUD with >= 80% free playfield, umlauts, inspector & 3-col Q/W/E
    if !smoke.stage_03_shot_taken && smoke.elapsed >= 8.0 {
        smoke.stage_03_shot_taken = true;
        info!("SmokeTest: capturing verify_03_ui.png");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("verify_03_ui.png"));
    }

    // Clean frame buffer after final screenshot before exiting
    if smoke.stage_03_shot_taken && !smoke.exit_requested {
        if capturing.is_empty() {
            smoke.post_shot_buffer_frames += 1;
            if smoke.post_shot_buffer_frames >= 20 {
                smoke.exit_requested = true;
                info!("SmokeTest: all 3 screenshots captured and flushed, exiting cleanly");
                app_exit.write(AppExit::Success);
            }
        } else {
            smoke.post_shot_buffer_frames = 0;
        }
    }
}
