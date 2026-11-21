use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use directories::ProjectDirs;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::types::*;

const EMBEDDED_BALANCE: &str = include_str!("../../config/balance.ron");
const EMBEDDED_ENEMIES: &str = include_str!("../../config/enemies.ron");
const EMBEDDED_WAVES: &str = include_str!("../../config/waves.ron");

const EMBEDDED_MAP_LORDAERON: &str = include_str!("../../config/maps/lordaeron_bastion.ron");
const EMBEDDED_MAP_WINTERMAUL: &str = include_str!("../../config/maps/wintermaul_glacier.ron");

const EMBEDDED_FACTION_ELEMENTAL: &str =
    include_str!("../../config/factions/elemental_circle.ron");
const EMBEDDED_FACTION_CITADEL: &str = include_str!("../../config/factions/citadel.ron");
const EMBEDDED_FACTION_DRAGON: &str = include_str!("../../config/factions/dragon_clan.ron");
const EMBEDDED_FACTION_FROSTGUARD: &str = include_str!("../../config/factions/frostguard.ron");
const EMBEDDED_FACTION_NECROPOLIS: &str = include_str!("../../config/factions/necropolis.ron");

pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_configs_startup)
            .add_systems(
                Update,
                (
                    handle_reload_hotkey.in_set(GameSet::InputSet),
                    handle_config_reload.in_set(GameSet::VfxUiSet),
                    handle_save_persistence.in_set(GameSet::VfxUiSet),
                ),
            )
            .add_systems(OnEnter(AppState::Victory), record_victory_on_enter)
            .add_systems(OnEnter(AppState::GameOver), record_defeat_on_enter);
    }
}

/// Resolves the `./config/` directory next to the executable (or current working directory fallback).
fn config_override_candidates(rel_path: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::with_capacity(2);
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        candidates.push(exe_dir.join("config").join(rel_path));
    }
    candidates.push(PathBuf::from("config").join(rel_path));
    candidates
}

fn load_ron_with_override<T: DeserializeOwned>(
    rel_path: &str,
    embedded: &str,
    overrides: &mut Vec<String>,
) -> Result<T, String> {
    for candidate in config_override_candidates(rel_path) {
        if candidate.is_file() {
            match fs::read_to_string(&candidate) {
                Ok(raw) => match ron::from_str::<T>(&raw) {
                    Ok(parsed) => {
                        let path_str = candidate.display().to_string();
                        info!("Loaded config override: {path_str}");
                        overrides.push(path_str);
                        return Ok(parsed);
                    }
                    Err(err) => {
                        warn!(
                            "Failed to parse external config override '{}' ({err}); falling back to embedded default.",
                            candidate.display()
                        );
                    }
                },
                Err(err) => {
                    warn!(
                        "Failed to read external config override '{}' ({err}); falling back to embedded default.",
                        candidate.display()
                    );
                }
            }
        }
    }

    ron::from_str::<T>(embedded)
        .map_err(|err| format!("Failed to parse embedded config '{rel_path}': {err}"))
}

/// Loads all 10 `.ron` configuration files (checking optional `./config/...` overrides first,
/// falling back cleanly to the embedded `include_str!` defaults).
pub fn load_all_configs() -> GameConfigs {
    let mut overrides = Vec::new();

    let balance = match load_ron_with_override::<BalanceConfig>(
        "balance.ron",
        EMBEDDED_BALANCE,
        &mut overrides,
    ) {
        Ok(cfg) => cfg,
        Err(err) => {
            error!("{err}");
            default_balance_config()
        }
    };

    let enemies = match load_ron_with_override::<HashMap<EnemyArchetypeId, EnemyConfig>>(
        "enemies.ron",
        EMBEDDED_ENEMIES,
        &mut overrides,
    ) {
        Ok(cfg) => cfg,
        Err(err) => {
            error!("{err}");
            default_enemies()
        }
    };

    let waves = match load_ron_with_override::<WaveTableConfig>(
        "waves.ron",
        EMBEDDED_WAVES,
        &mut overrides,
    ) {
        Ok(cfg) => cfg,
        Err(err) => {
            error!("{err}");
            default_waves()
        }
    };

    let mut maps = default_maps();
    for (rel_path, embedded) in [
        ("maps/lordaeron_bastion.ron", EMBEDDED_MAP_LORDAERON),
        ("maps/wintermaul_glacier.ron", EMBEDDED_MAP_WINTERMAUL),
    ] {
        match load_ron_with_override::<MapConfig>(rel_path, embedded, &mut overrides) {
            Ok(map_cfg) => {
                maps.insert(map_cfg.id, map_cfg);
            }
            Err(err) => error!("{err}"),
        }
    }

    let mut factions = default_factions();
    for (rel_path, embedded) in [
        ("factions/elemental_circle.ron", EMBEDDED_FACTION_ELEMENTAL),
        ("factions/citadel.ron", EMBEDDED_FACTION_CITADEL),
        ("factions/dragon_clan.ron", EMBEDDED_FACTION_DRAGON),
        ("factions/frostguard.ron", EMBEDDED_FACTION_FROSTGUARD),
        ("factions/necropolis.ron", EMBEDDED_FACTION_NECROPOLIS),
    ] {
        match load_ron_with_override::<FactionConfig>(rel_path, embedded, &mut overrides) {
            Ok(faction_cfg) => {
                factions.insert(faction_cfg.id, faction_cfg);
            }
            Err(err) => error!("{err}"),
        }
    }

    GameConfigs {
        balance,
        maps,
        factions,
        enemies,
        waves,
        loaded_from_override: overrides,
    }
}

/// Resolves the save directory:
/// - `./save/` next to the executable (or cwd) if `portable.flag` exists next to the exe
/// - otherwise `directories::ProjectDirs::from("com", "Aethermaul", "AethermaulTD")`
fn resolve_save_dir() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
        && exe_dir.join("portable.flag").is_file()
    {
        return exe_dir.join("save");
    }

    if Path::new("portable.flag").is_file() {
        return PathBuf::from("save");
    }

    if let Some(proj_dirs) = ProjectDirs::from("com", "Aethermaul", "AethermaulTD") {
        return proj_dirs.data_local_dir().to_path_buf();
    }

    PathBuf::from("save")
}

fn load_ron_file_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    if !path.is_file() {
        return T::default();
    }
    match fs::read_to_string(path) {
        Ok(raw) => match ron::from_str::<T>(&raw) {
            Ok(parsed) => parsed,
            Err(err) => {
                warn!(
                    "Failed to parse persistence file '{}' ({err}); using defaults.",
                    path.display()
                );
                T::default()
            }
        },
        Err(err) => {
            warn!(
                "Failed to read persistence file '{}' ({err}); using defaults.",
                path.display()
            );
            T::default()
        }
    }
}

fn save_ron_pretty<T: Serialize>(path: &Path, value: &T) {
    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        warn!(
            "Failed to create save directory '{}': {err}",
            parent.display()
        );
        return;
    }

    let pretty = ron::ser::PrettyConfig::new()
        .depth_limit(4)
        .separate_tuple_members(true)
        .enumerate_arrays(false);
    match ron::ser::to_string_pretty(value, pretty) {
        Ok(serialized) => {
            if let Err(err) = fs::write(path, serialized) {
                warn!("Failed to write '{}': {err}", path.display());
            }
        }
        Err(err) => {
            warn!("Failed to serialize '{}': {err}", path.display());
        }
    }
}

fn load_configs_startup(
    mut configs: ResMut<GameConfigs>,
    mut settings: ResMut<GameSettings>,
    mut scores: ResMut<ProfileScores>,
) {
    *configs = load_all_configs();

    let save_dir = resolve_save_dir();
    *settings = load_ron_file_or_default::<GameSettings>(&save_dir.join("settings.ron"));
    *scores = load_ron_file_or_default::<ProfileScores>(&save_dir.join("profile_scores.ron"));

    info!(
        "Loaded GameConfigs ({} maps, {} factions, {} enemies, {} waves, {} external overrides). Save dir: {}",
        configs.maps.len(),
        configs.factions.len(),
        configs.enemies.len(),
        configs.waves.waves.len(),
        configs.loaded_from_override.len(),
        save_dir.display()
    );
}

fn handle_reload_hotkey(
    keys: Res<ButtonInput<KeyCode>>,
    mut reload_writer: MessageWriter<ConfigReloadMsg>,
) {
    if keys.just_pressed(KeyCode::F5) {
        reload_writer.write(ConfigReloadMsg);
    }
}

fn handle_config_reload(
    mut reload_reader: MessageReader<ConfigReloadMsg>,
    mut configs: ResMut<GameConfigs>,
    mut economy: ResMut<EconomyState>,
) {
    let mut triggered = false;
    for _ in reload_reader.read() {
        triggered = true;
    }
    if triggered {
        *configs = load_all_configs();
        economy.status_banner = Some((
            format!(
                "Configs neu geladen ({} Overrides)",
                configs.loaded_from_override.len()
            ),
            2.5,
        ));
        info!("Reloaded GameConfigs via ConfigReloadMsg (F5).");
    }
}

fn handle_save_persistence(
    mut save_reader: MessageReader<SavePersistenceMsg>,
    settings: Res<GameSettings>,
    scores: Res<ProfileScores>,
) {
    let mut requested = false;
    for _ in save_reader.read() {
        requested = true;
    }
    if requested {
        let save_dir = resolve_save_dir();
        save_ron_pretty(&save_dir.join("settings.ron"), &*settings);
        save_ron_pretty(&save_dir.join("profile_scores.ron"), &*scores);
    }
}

fn update_profile_score(
    scores: &mut ProfileScores,
    match_cfg: &MatchConfig,
    wave: &WaveState,
    economy: &EconomyState,
    is_victory: bool,
) {
    let key = (match_cfg.map_id, match_cfg.faction_id, match_cfg.difficulty);
    let entry = scores.records.entry(key).or_default();

    entry.best_wave = entry.best_wave.max(wave.current_wave);
    entry.best_score = entry.best_score.max(economy.score);
    entry.total_kills = entry.total_kills.saturating_add(economy.total_kills);
    if is_victory {
        entry.victories = entry.victories.saturating_add(1);
        let clear_secs = economy.match_elapsed_secs;
        entry.fastest_clear_secs = Some(match entry.fastest_clear_secs {
            Some(prev) => prev.min(clear_secs),
            None => clear_secs,
        });
    }
}

fn record_victory_on_enter(
    match_cfg: Res<MatchConfig>,
    wave: Res<WaveState>,
    economy: Res<EconomyState>,
    mut scores: ResMut<ProfileScores>,
    mut save_writer: MessageWriter<SavePersistenceMsg>,
) {
    update_profile_score(&mut scores, &match_cfg, &wave, &economy, true);
    save_writer.write(SavePersistenceMsg);
}

fn record_defeat_on_enter(
    match_cfg: Res<MatchConfig>,
    wave: Res<WaveState>,
    economy: Res<EconomyState>,
    mut scores: ResMut<ProfileScores>,
    mut save_writer: MessageWriter<SavePersistenceMsg>,
) {
    update_profile_score(&mut scores, &match_cfg, &wave, &economy, false);
    save_writer.write(SavePersistenceMsg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_balance_ron_parses_cleanly() {
        let balance: BalanceConfig =
            ron::from_str(EMBEDDED_BALANCE).expect("balance.ron must deserialize");
        assert_eq!(balance.difficulties.len(), 3);
        assert!(balance.difficulties.contains_key(&Difficulty::Normal));
        assert!(balance.difficulties.contains_key(&Difficulty::Hard));
        assert!(balance.difficulties.contains_key(&Difficulty::Nightmare));
        assert_eq!(balance.interest_cap, 250);
    }

    #[test]
    fn embedded_maps_ron_parse_cleanly() {
        let lordaeron: MapConfig =
            ron::from_str(EMBEDDED_MAP_LORDAERON).expect("lordaeron_bastion.ron must deserialize");
        assert_eq!(lordaeron.id, MapId::LordaeronBastion);
        assert_eq!(lordaeron.width, 28);
        assert_eq!(lordaeron.height, 20);
        assert_eq!(lordaeron.spawns.len(), 1);
        assert_eq!(lordaeron.checkpoints.len(), 3);

        let wintermaul: MapConfig = ron::from_str(EMBEDDED_MAP_WINTERMAUL)
            .expect("wintermaul_glacier.ron must deserialize");
        assert_eq!(wintermaul.id, MapId::WintermaulGlacier);
        assert_eq!(wintermaul.width, 30);
        assert_eq!(wintermaul.height, 22);
        assert_eq!(wintermaul.spawns.len(), 2);
        assert_eq!(wintermaul.checkpoints.len(), 2);
    }

    #[test]
    fn embedded_factions_ron_parse_cleanly_and_have_all_towers() {
        let expected: [(&str, FactionId, usize); 5] = [
            (EMBEDDED_FACTION_ELEMENTAL, FactionId::ElementalCircle, 10),
            (EMBEDDED_FACTION_CITADEL, FactionId::Citadel, 9),
            (EMBEDDED_FACTION_DRAGON, FactionId::DragonClan, 8),
            (EMBEDDED_FACTION_FROSTGUARD, FactionId::Frostguard, 7),
            (EMBEDDED_FACTION_NECROPOLIS, FactionId::Necropolis, 7),
        ];

        for (raw, expected_id, expected_count) in expected {
            let cfg: FactionConfig =
                ron::from_str(raw).unwrap_or_else(|e| panic!("faction {expected_id:?}: {e}"));
            assert_eq!(cfg.id, expected_id);
            assert_eq!(
                cfg.towers.len(),
                expected_count,
                "unexpected tower count for {expected_id:?}"
            );
            for root_id in &cfg.root_buildable_towers {
                assert!(
                    cfg.towers.contains_key(root_id),
                    "missing root tower {root_id} in {expected_id:?}"
                );
            }
            for (tower_id, tower) in &cfg.towers {
                assert_eq!(tower_id, &tower.id);
                for upg in &tower.upgrades_to {
                    assert!(
                        cfg.towers.contains_key(upg),
                        "missing upgrade target {upg} from {tower_id} in {expected_id:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn embedded_enemies_ron_parses_all_14_archetypes() {
        let enemies: HashMap<EnemyArchetypeId, EnemyConfig> =
            ron::from_str(EMBEDDED_ENEMIES).expect("enemies.ron must deserialize");
        assert_eq!(enemies.len(), 14);
        let all = [
            EnemyArchetypeId::RatSwarm,
            EnemyArchetypeId::GoblinRaider,
            EnemyArchetypeId::BatFlyer,
            EnemyArchetypeId::SlimeSplitter,
            EnemyArchetypeId::MiniSlime,
            EnemyArchetypeId::VampireLord,
            EnemyArchetypeId::StoneGolem,
            EnemyArchetypeId::Gargoyle,
            EnemyArchetypeId::BossRatKing,
            EnemyArchetypeId::BossSlimeTitan,
            EnemyArchetypeId::BossBloodCount,
            EnemyArchetypeId::BossObsidianWing,
            EnemyArchetypeId::BossGolemColossus,
            EnemyArchetypeId::BossAetherDreadlord,
        ];
        for id in all {
            assert!(enemies.contains_key(&id), "missing enemy {id:?}");
        }
    }

    #[test]
    fn embedded_waves_ron_parses_all_30_waves() {
        let table: WaveTableConfig =
            ron::from_str(EMBEDDED_WAVES).expect("waves.ron must deserialize");
        assert_eq!(table.waves.len(), 30);
        for (idx, w) in table.waves.iter().enumerate() {
            assert_eq!(w.wave_number as usize, idx + 1);
            assert!(!w.groups.is_empty());
        }
    }

    #[test]
    fn roundtrip_settings_and_profile_scores_ron() {
        let settings = GameSettings::default();
        let encoded = ron::to_string(&settings).expect("serialize GameSettings");
        let decoded: GameSettings = ron::from_str(&encoded).expect("deserialize GameSettings");
        assert_eq!(decoded.muted, settings.muted);

        let mut scores = ProfileScores::default();
        scores.records.insert(
            (
                MapId::LordaeronBastion,
                FactionId::ElementalCircle,
                Difficulty::Normal,
            ),
            ScoreRecord {
                best_wave: 30,
                best_score: 12345,
                victories: 1,
                total_kills: 450,
                fastest_clear_secs: Some(612.5),
            },
        );
        let encoded_scores = ron::to_string(&scores).expect("serialize ProfileScores");
        let decoded_scores: ProfileScores =
            ron::from_str(&encoded_scores).expect("deserialize ProfileScores");
        assert_eq!(decoded_scores.records.len(), 1);
    }
}
