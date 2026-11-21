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
    mut flow: ResMut<FlowField>,
    mut wave: ResMut<WaveState>,
    mut selection: ResMut<SelectionState>,
    mut build_writer: MessageWriter<BuildTowerRequest>,
    mut send_wave_writer: MessageWriter<SendWaveEarlyRequest>,
    towers: Query<(Entity, &Tower)>,
    capturing: Query<Entity, With<Capturing>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    // Advance smoke clock smoothly (even on slow software renderers, step >= 1/30s)
    let step = time.delta_secs().clamp(1.0 / 60.0, 0.12);
    smoke.elapsed += step;

    // Step 1 (t >= 0.2s): Build classic serpentine maze on Lordaeron-Bastion & enable path preview
    if !smoke.stage_01_maze_built && smoke.elapsed >= 0.2 {
        smoke.stage_01_maze_built = true;
        flow.show_path_overlay = true;
        wave.next_wave_has_air = true;

        let maze_coords = [
            // Baffle 1 (x=6, z=0..=4)
            TileCoord::new(6, 0),
            TileCoord::new(6, 1),
            TileCoord::new(6, 2),
            TileCoord::new(6, 3),
            TileCoord::new(6, 4),
            // Baffle 2 (x=10, z=2..=7)
            TileCoord::new(10, 2),
            TileCoord::new(10, 3),
            TileCoord::new(10, 4),
            TileCoord::new(10, 5),
            TileCoord::new(10, 6),
            TileCoord::new(10, 7),
            // Baffle 3 (x=15, z=0..=4)
            TileCoord::new(15, 0),
            TileCoord::new(15, 1),
            TileCoord::new(15, 2),
            TileCoord::new(15, 3),
            TileCoord::new(15, 4),
            // Baffle 4 (x=19, z=2..=7)
            TileCoord::new(19, 2),
            TileCoord::new(19, 3),
            TileCoord::new(19, 4),
            TileCoord::new(19, 5),
            TileCoord::new(19, 6),
            TileCoord::new(19, 7),
            // Mid-lane maze baffles
            TileCoord::new(12, 9),
            TileCoord::new(12, 10),
            TileCoord::new(12, 11),
            TileCoord::new(17, 10),
            TileCoord::new(17, 11),
            TileCoord::new(17, 12),
        ];

        for coord in maze_coords {
            build_writer.write(BuildTowerRequest {
                coord,
                tower_id: "arcane_pillar".to_string(),
                faction: FactionId::ElementalCircle,
                free: true,
            });
        }
        info!("SmokeTest: requested serpentine maze construction");
    }

    // Screenshot 1 (t >= 2.0s): Maze + ground & air path preview
    if !smoke.stage_01_shot_taken && smoke.elapsed >= 2.0 {
        smoke.stage_01_shot_taken = true;
        info!("SmokeTest: capturing verify_01_maze.png");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("verify_01_maze.png"));
    }

    // Step 2 (t >= 2.5s): Build multi-faction & dual-element towers + start waves
    if !smoke.stage_02_combat_built && smoke.elapsed >= 2.5 {
        smoke.stage_02_combat_built = true;
        flow.show_path_overlay = false;

        let showcase_towers: [(TileCoord, FactionId, &str); 11] = [
            // Elemental Circle dual elements & elements right near Spawn->Baffle 1 corridor
            (
                TileCoord::new(4, 2),
                FactionId::ElementalCircle,
                "fire_obelisk",
            ),
            (
                TileCoord::new(5, 4),
                FactionId::ElementalCircle,
                "steam_condenser",
            ),
            (
                TileCoord::new(7, 3),
                FactionId::ElementalCircle,
                "plasma_core",
            ),
            (
                TileCoord::new(8, 2),
                FactionId::ElementalCircle,
                "storm_pillar",
            ),
            (
                TileCoord::new(8, 5),
                FactionId::ElementalCircle,
                "water_fountain",
            ),
            // Citadel
            (
                TileCoord::new(4, 4),
                FactionId::Citadel,
                "gatling_ballista",
            ),
            (
                TileCoord::new(7, 6),
                FactionId::Citadel,
                "volley_bastion",
            ),
            // Dragon Clan
            (
                TileCoord::new(8, 0),
                FactionId::DragonClan,
                "elder_fire_dragon",
            ),
            // Frostguard
            (
                TileCoord::new(11, 4),
                FactionId::Frostguard,
                "blizzard_spire",
            ),
            // Necropolis
            (
                TileCoord::new(11, 6),
                FactionId::Necropolis,
                "plague_ziggurat",
            ),
            (
                TileCoord::new(13, 3),
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

        // Trigger Wave 1 immediately so combat is raging by t=6.0s
        send_wave_writer.write(SendWaveEarlyRequest);
        info!("SmokeTest: built multi-faction & dual-element showcase and started Wave 1");
    }

    // Screenshot 2 (t >= 6.0s): Active combat with projectiles, effects & 3D HP bars
    if !smoke.stage_02_shot_taken && smoke.elapsed >= 6.0 {
        smoke.stage_02_shot_taken = true;
        info!("SmokeTest: capturing verify_02_combat.png");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("verify_02_combat.png"));
    }

    // Step 3 (t >= 6.6s): Select a dual-element tower to verify contextual WC3 bottom HUD
    if !smoke.stage_03_ui_selected && smoke.elapsed >= 6.6 {
        smoke.stage_03_ui_selected = true;
        if let Some((entity, _)) = towers
            .iter()
            .find(|(_, t)| t.id == "plasma_core" || t.id == "storm_pillar")
            .or_else(|| towers.iter().next())
        {
            selection.selected_towers = vec![entity];
            selection.selected_enemy = None;
            info!("SmokeTest: selected showcase tower for UI verification");
        }
    }

    // Screenshot 3 (t >= 8.0s): WC3 HUD with >= 80% free playfield
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
