//! Frozen ECS contracts, shared resources, components, messages, configuration schemas,
//! and system-set pipeline for **Aethermaul TD**.
//!
//! All plugins communicate exclusively through these shared types so parallel workers
//! have strict module isolation and zero runtime entity-despawn panics.

#![allow(dead_code)]

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

// ============================================================================
// Constants
// ============================================================================

/// Size of a single grid tile in world units.
pub const TILE_SIZE: f32 = 1.0;

/// Flight altitude for flying enemies (`MovementDomain::Air`) and air path preview.
pub const AIR_ALTITUDE: f32 = 2.6;

/// Maximum simultaneous 3D particles for performance budgeting.
pub const MAX_ACTIVE_PARTICLES: usize = 350;

/// Maximum simultaneous floating popups ("nicht überladen").
pub const MAX_ACTIVE_POPUPS: usize = 40;

// ============================================================================
// States & Deterministic 10-Stage SystemSet Pipeline
// ============================================================================

#[derive(States, Debug, Clone, Copy, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum AppState {
    #[default]
    MainMenu,
    FractionSelect,
    Playing,
    Paused,
    Victory,
    GameOver,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum WavePhase {
    #[default]
    BuildPrep,
    WaveActive,
}

#[derive(Resource, Debug, Clone, Copy, Eq, PartialEq, Default, Serialize, Deserialize)]
pub enum GameSpeed {
    Paused,
    #[default]
    Normal,
    Fast,
    Ultra,
}

impl GameSpeed {
    pub fn multiplier(self) -> f32 {
        match self {
            Self::Paused => 0.0,
            Self::Normal => 1.0,
            Self::Fast => 2.0,
            Self::Ultra => 3.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Paused => "||",
            Self::Normal => "1x",
            Self::Fast => "2x",
            Self::Ultra => "3x",
        }
    }
}

/// Fixed 10-stage simulation pipeline executed in strict order each frame while playing.
/// Despawns happen ONLY in `CleanupSet` via `PendingDespawn`, preventing stale-entity panics.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet {
    InputSet,
    BuildSet,
    PathfindingSet,
    SpawnSet,
    MovementSet,
    TargetingSet,
    CombatSet,
    LeakEconomySet,
    CleanupSet,
    VfxUiSet,
}

// ============================================================================
// Core Domain Enums
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum FactionId {
    #[default]
    ElementalCircle,
    Citadel,
    DragonClan,
    Frostguard,
    Necropolis,
}

impl FactionId {
    pub const ALL: [Self; 5] = [
        Self::ElementalCircle,
        Self::Citadel,
        Self::DragonClan,
        Self::Frostguard,
        Self::Necropolis,
    ];

    pub fn id_str(self) -> &'static str {
        match self {
            Self::ElementalCircle => "elemental_circle",
            Self::Citadel => "citadel",
            Self::DragonClan => "dragon_clan",
            Self::Frostguard => "frostguard",
            Self::Necropolis => "necropolis",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::ElementalCircle => "Elementar-Zirkel",
            Self::Citadel => "Zitadelle",
            Self::DragonClan => "Drachenklan",
            Self::Frostguard => "Frostwacht",
            Self::Necropolis => "Nekropolis",
        }
    }

    pub fn currency_name(self) -> &'static str {
        match self {
            Self::ElementalCircle => "Elementar-Essenzen",
            Self::Citadel => "Handwerkskerne",
            Self::DragonClan => "Glutseelen",
            Self::Frostguard => "Frostsplitter",
            Self::Necropolis => "Seelen",
        }
    }

    pub fn short_tagline(self) -> &'static str {
        match self {
            Self::ElementalCircle => "Feuer, Wasser, Blitz & Dual-Elemente (Dampf/Plasma/Sturm)",
            Self::Citadel => "Pfeil-Tech (Schnellfeuer/Multishot/Mörser) & Gold-Wirtschaft",
            Self::DragonClan => "AoE-Drachenatem, Burn-DoT & starke Late-Game-Skalierung",
            Self::Frostguard => "Slow, Freeze-Stacks & +80-110% Shatter-Crits",
            Self::Necropolis => "Seelen pro Kill, Armor-Shred & 95% Anti-Heal-Seuchen",
        }
    }

    pub fn accent_color(self) -> Color {
        match self {
            Self::ElementalCircle => Color::srgb(0.28, 0.72, 0.98),
            Self::Citadel => Color::srgb(0.92, 0.76, 0.28),
            Self::DragonClan => Color::srgb(0.95, 0.36, 0.18),
            Self::Frostguard => Color::srgb(0.45, 0.88, 1.0),
            Self::Necropolis => Color::srgb(0.48, 0.92, 0.36),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum MapId {
    #[default]
    LordaeronBastion,
    WintermaulGlacier,
}

impl MapId {
    pub const ALL: [Self; 2] = [Self::LordaeronBastion, Self::WintermaulGlacier];

    pub fn id_str(self) -> &'static str {
        match self {
            Self::LordaeronBastion => "lordaeron_bastion",
            Self::WintermaulGlacier => "wintermaul_glacier",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::LordaeronBastion => "Lordaeron-Bastion (28x20)",
            Self::WintermaulGlacier => "Wintermaul-Gletscher (30x22)",
        }
    }

    pub fn biome(self) -> BiomeKind {
        match self {
            Self::LordaeronBastion => BiomeKind::LordaeronGrass,
            Self::WintermaulGlacier => BiomeKind::WintermaulIce,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BiomeKind {
    #[default]
    LordaeronGrass,
    WintermaulIce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Difficulty {
    #[default]
    Normal,
    Hard,
    Nightmare,
}

impl Difficulty {
    pub const ALL: [Self; 3] = [Self::Normal, Self::Hard, Self::Nightmare];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Hard => "Schwer",
            Self::Nightmare => "Albtraum",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum MovementDomain {
    #[default]
    Ground,
    Air,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TargetMask {
    GroundOnly,
    AirOnly,
    #[default]
    GroundAndAir,
}

impl TargetMask {
    pub fn can_target(self, domain: MovementDomain) -> bool {
        matches!(
            (self, domain),
            (Self::GroundOnly, MovementDomain::Ground)
                | (Self::AirOnly, MovementDomain::Air)
                | (Self::GroundAndAir, _)
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::GroundOnly => "Boden",
            Self::AirOnly => "Luft",
            Self::GroundAndAir => "Boden + Luft",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TargetPriority {
    #[default]
    First,
    Strongest,
    Weakest,
    Closest,
    AirPriority,
}

impl TargetPriority {
    pub fn next(self) -> Self {
        match self {
            Self::First => Self::Strongest,
            Self::Strongest => Self::Weakest,
            Self::Weakest => Self::Closest,
            Self::Closest => Self::AirPriority,
            Self::AirPriority => Self::First,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::First => "Erster",
            Self::Strongest => "Stärkster",
            Self::Weakest => "Schwächster",
            Self::Closest => "Nächster",
            Self::AirPriority => "Luft-Prio",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TileCoord {
    pub x: i32,
    pub z: i32,
}

impl TileCoord {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TileKind {
    #[default]
    Buildable,
    BlockedTerrain,
    Spawn(u8),
    Checkpoint(u8),
    Goal,
    Tower(Entity),
}

impl TileKind {
    pub fn is_walkable(self) -> bool {
        matches!(
            self,
            Self::Buildable | Self::Spawn(_) | Self::Checkpoint(_) | Self::Goal
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildBlockReason {
    OutOfBounds,
    NotBuildable,
    OccupiedByTower,
    OccupiedByEnemy,
    ReclaimCooldown,
    BlocksPath,
    TrapsEnemy,
    InsufficientFunds,
    MaxCountReached,
}

impl BuildBlockReason {
    pub fn message(self) -> &'static str {
        match self {
            Self::OutOfBounds => "Außerhalb der Karte",
            Self::NotBuildable => "Nicht bebaubar",
            Self::OccupiedByTower => "Bereits bebaut",
            Self::OccupiedByEnemy => "Gegner blockiert Feld",
            Self::ReclaimCooldown => "Bau-Cooldown aktiv (1.5s)",
            Self::BlocksPath => "Blockiert Gegnerpfad!",
            Self::TrapsEnemy => "Schließt Gegner ein!",
            Self::InsufficientFunds => "Nicht genug Gold/Währung",
            Self::MaxCountReached => "Maximale Anzahl erreicht",
        }
    }
}

// ============================================================================
// Combat, Tower & Enemy Config Enums
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AttackKind {
    Projectile {
        speed: f32,
        splash_radius: f32,
        is_heavy: bool,
    },
    MultiShot {
        count: u8,
        speed: f32,
        bounce_once: bool,
    },
    ChainLightning {
        bounces: u8,
        decay_per_bounce: f32,
        jump_range: f32,
    },
    Beam {
        ramp_armor_shred: f32,
    },
    ConeAoe {
        radius: f32,
    },
    SupportAura {
        aura_radius: f32,
        dmg_bonus: f32,
        bounty_bonus: f32,
        soul_mult: f32,
        passive_gold: f32,
        interest_bonus: f32,
        interest_cap_bonus: u32,
        core_every_waves: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OnHitEffect {
    Burn {
        dps: f32,
        duration: f32,
        anti_heal: f32,
        spread_on_death: bool,
    },
    Poison {
        dps: f32,
        duration: f32,
        anti_heal: f32,
        puddle_on_death: bool,
    },
    Slow {
        percent: f32,
        duration: f32,
        frost_stacks: u8,
    },
    FreezeChance {
        chance: f32,
        duration: f32,
    },
    ArmorShred {
        amount: f32,
        max_stack: f32,
        duration: f32,
    },
    MiniStun {
        duration: f32,
        immunity: f32,
    },
    Curse {
        dmg_amp: f32,
        duration: f32,
    },
    Shatter {
        bonus_mult: f32,
        nova_radius: f32,
    },
    Crit {
        chance: f32,
        mult: f32,
    },
    DeathExplosion {
        max_hp_pct: f32,
        radius: f32,
    },
    ArmorPen {
        every_nth: u8,
        ignore_pct: f32,
    },
    AirKnockback {
        distance: f32,
    },
    SlowAura {
        radius: f32,
        percent: f32,
    },
    RampAttackSpeed {
        max_bonus_aspd: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CurrencyRule {
    EveryNWavesAndBoss {
        every_waves: u32,
        wave_amount: u32,
        boss_amount: u32,
    },
    EveryNWavesAndTreasury {
        every_waves: u32,
        wave_amount: u32,
    },
    BurnKillsAndBoss {
        burn_kill_amount: u32,
        normal_kills_per_unit: u32,
        boss_amount: u32,
    },
    SlowedKillsAndFreeze {
        slowed_kill_amount: u32,
        freeze_proc_amount: u32,
    },
    EveryKill {
        normal_amount: u32,
        elite_amount: u32,
        boss_amount: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TowerMeshRecipe {
    // Elemental Circle
    #[default]
    ArcanePillar,
    FireObelisk,
    WaterFountain,
    LightningCoil,
    InfernoSpire,
    GlacierSource,
    ThunderPylon,
    SteamCondenser,
    PlasmaCore,
    StormPillar,
    // Citadel
    Watchtower,
    RapidFireTower,
    GatlingBallista,
    MultishotTower,
    VolleyBastion,
    HeavyCannon,
    SiegeMortar,
    FlakHunter,
    GuildTreasury,
    // Dragon Clan
    BroodNest,
    RedDragonRoost,
    ElderFireDragon,
    BronzeDragonSpire,
    TimeDragonSanctum,
    StormDragonPerch,
    SkyWyrmPeak,
    GoldHoardShrine,
    // Frostguard
    IcePillar,
    FrostSpearThrower,
    GlacierSniper,
    BlizzardObelisk,
    BlizzardSpire,
    IceShardCatapult,
    NorthwindTower,
    // Necropolis
    BoneSpire,
    PlagueZiggurat,
    PestCathedral,
    DecayObelisk,
    LichThron,
    BansheeTower,
    SoulCrypt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum EnemyArchetypeId {
    RatSwarm,
    #[default]
    GoblinRaider,
    BatFlyer,
    SlimeSplitter,
    MiniSlime,
    VampireLord,
    StoneGolem,
    Gargoyle,
    BossRatKing,
    BossSlimeTitan,
    BossBloodCount,
    BossObsidianWing,
    BossGolemColossus,
    BossAetherDreadlord,
}

impl EnemyArchetypeId {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::RatSwarm => "Ratten-Schwarm",
            Self::GoblinRaider => "Goblin-Plünderer",
            Self::BatFlyer => "Fledermaus (Luft)",
            Self::SlimeSplitter => "Schleim-Splitter",
            Self::MiniSlime => "Mini-Schleim",
            Self::VampireLord => "Vampir-Lord",
            Self::StoneGolem => "Steingolem",
            Self::Gargoyle => "Gargoyle (Luft)",
            Self::BossRatKing => "BOSS: Rattenkönig",
            Self::BossSlimeTitan => "BOSS: Uralter Schleim-Titan",
            Self::BossBloodCount => "BOSS: Blutgraf",
            Self::BossObsidianWing => "BOSS: Obsidianschwinge (Luft)",
            Self::BossGolemColossus => "BOSS: Golem-Koloss",
            Self::BossAetherDreadlord => "ENDBOSS: Aether-Dreadlord",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BossAbilityKind {
    #[default]
    None,
    SummonRats,
    SplitThree,
    BloodRegenPulse,
    StoneShield,
    SummonVampiresAndBats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SfxEventKind {
    ArrowShot,
    FireCast,
    Explosion,
    IceHit,
    Freeze,
    LightningZap,
    PoisonCast,
    BuildTower,
    UpgradeTower,
    SellTower,
    GoldCoin,
    LimitWarning,
    WaveStart,
    PortalWarp,
    VictoryFanfare,
    DefeatHorn,
    UiClick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VfxKind {
    ImpactSpark,
    FireExplosion,
    IceShatter,
    LightningArc,
    PoisonCloud,
    SteamCloud,
    PortalWarp,
    BuildDust,
    UpgradeBurst,
    EnemyDeath,
}

// ============================================================================
// Configuration Structs (Deserialized from config/*.ron + Embedded Defaults)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyConfig {
    pub start_gold: u32,
    pub hp_mult: f32,
    pub armor_add: f32,
    pub bounty_mult: f32,
    pub max_slots: u32,
    pub score_mult: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceConfig {
    pub prep_time_first_wave_secs: f32,
    pub prep_time_secs: f32,
    pub passive_gold_per_sec: f32,
    pub wave_bonus_base: u32,
    pub wave_bonus_per_wave: u32,
    pub interest_rate: f32,
    pub interest_cap: u32,
    pub early_send_base_per_sec: f32,
    pub early_send_wave_scale: f32,
    pub sell_refund_same_prep: f32,
    pub sell_refund_prep: f32,
    pub sell_refund_active_wave: f32,
    pub reclaim_cooldown_secs: f32,
    pub anti_juggle_reroute_threshold: u8,
    pub anti_juggle_enrage_secs: f32,
    pub anti_juggle_speed_mult: f32,
    pub anti_juggle_cooldown_secs: f32,
    pub limit_grace_period_secs: f32,
    pub max_slow_normal: f32,
    pub max_slow_boss: f32,
    pub max_armor_shred: f32,
    pub difficulties: HashMap<Difficulty, DifficultyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapZone {
    pub origin: TileCoord,
    pub width: i32,
    pub height: i32,
}

impl MapZone {
    pub fn tiles(&self) -> Vec<TileCoord> {
        let mut out = Vec::with_capacity((self.width * self.height).max(0) as usize);
        for dz in 0..self.height {
            for dx in 0..self.width {
                out.push(TileCoord::new(self.origin.x + dx, self.origin.z + dz));
            }
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapConfig {
    pub id: MapId,
    pub display_name: String,
    pub biome: BiomeKind,
    pub width: i32,
    pub height: i32,
    pub spawns: Vec<MapZone>,
    pub checkpoints: Vec<MapZone>,
    pub goal: MapZone,
    pub blocked_tiles: Vec<TileCoord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TowerConfig {
    pub id: String,
    pub name: String,
    pub hotkey: String,
    pub description: String,
    pub tier: u8,
    pub gold_cost: u32,
    pub special_cost: u32,
    pub max_count: Option<u32>,
    pub target_mask: TargetMask,
    pub damage: f32,
    pub attack_speed: f32,
    pub range: f32,
    pub air_bonus_mult: f32,
    pub attack_kind: AttackKind,
    pub on_hit_effects: Vec<OnHitEffect>,
    pub upgrades_to: Vec<String>,
    pub mesh_recipe: TowerMeshRecipe,
    pub projectile_color: (f32, f32, f32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionConfig {
    pub id: FactionId,
    pub display_name: String,
    pub currency_name: String,
    pub starting_currency: u32,
    pub currency_rule: CurrencyRule,
    pub primary_color: (f32, f32, f32),
    pub accent_color: (f32, f32, f32),
    pub root_buildable_towers: Vec<String>,
    pub towers: HashMap<String, TowerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnemyConfig {
    pub id: EnemyArchetypeId,
    pub domain: MovementDomain,
    pub base_hp: f32,
    pub base_speed: f32,
    pub base_armor: f32,
    pub slot_weight: f32,
    pub base_bounty: u32,
    pub is_boss: bool,
    pub is_elite: bool,
    pub regen_pct_per_sec: f32,
    pub split_into: Option<(EnemyArchetypeId, u8, f32)>,
    pub boss_ability: BossAbilityKind,
    pub scale: f32,
    pub primary_color: (f32, f32, f32),
    pub accent_color: (f32, f32, f32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveGroupConfig {
    pub archetype: EnemyArchetypeId,
    pub count: u32,
    pub hp_override: Option<f32>,
    pub bounty_override: Option<u32>,
    pub spawn_interval: f32,
    pub start_delay: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveDefConfig {
    pub wave_number: u32,
    pub title: String,
    pub is_boss_wave: bool,
    pub has_air: bool,
    pub groups: Vec<WaveGroupConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveTableConfig {
    pub waves: Vec<WaveDefConfig>,
    pub endless_hp_growth: f32,
    pub endless_armor_per_wave: f32,
    pub endless_speed_per_wave: f32,
    pub endless_speed_cap: f32,
    pub endless_bounty_growth: f32,
}

#[derive(Resource, Debug, Clone)]
pub struct GameConfigs {
    pub balance: BalanceConfig,
    pub maps: HashMap<MapId, MapConfig>,
    pub factions: HashMap<FactionId, FactionConfig>,
    pub enemies: HashMap<EnemyArchetypeId, EnemyConfig>,
    pub waves: WaveTableConfig,
    pub loaded_from_override: Vec<String>,
}

impl GameConfigs {
    pub fn difficulty_cfg(&self, difficulty: Difficulty) -> DifficultyConfig {
        self.balance
            .difficulties
            .get(&difficulty)
            .cloned()
            .unwrap_or(DifficultyConfig {
                start_gold: 120,
                hp_mult: 1.0,
                armor_add: 0.0,
                bounty_mult: 1.0,
                max_slots: 45,
                score_mult: 1.0,
            })
    }

    pub fn get_map(&self, id: MapId) -> Option<&MapConfig> {
        self.maps.get(&id)
    }

    pub fn get_faction(&self, id: FactionId) -> Option<&FactionConfig> {
        self.factions.get(&id)
    }

    pub fn get_tower(&self, faction: FactionId, tower_id: &str) -> Option<&TowerConfig> {
        self.factions
            .get(&faction)
            .and_then(|f| f.towers.get(tower_id))
    }

    pub fn get_enemy(&self, id: EnemyArchetypeId) -> Option<&EnemyConfig> {
        self.enemies.get(&id)
    }
}

// ============================================================================
// Runtime Resources
// ============================================================================

#[derive(Resource, Debug, Clone)]
pub struct MatchConfig {
    pub map_id: MapId,
    pub faction_id: FactionId,
    pub difficulty: Difficulty,
    pub endless_active: bool,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            map_id: MapId::LordaeronBastion,
            faction_id: FactionId::ElementalCircle,
            difficulty: Difficulty::Normal,
            endless_active: false,
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct GridMap {
    pub map_id: MapId,
    pub biome: BiomeKind,
    pub width: i32,
    pub height: i32,
    pub tiles: Vec<TileKind>,
    pub reclaim_timers: Vec<f32>,
    pub spawns: Vec<Vec<TileCoord>>,
    pub checkpoints: Vec<Vec<TileCoord>>,
    pub goals: Vec<TileCoord>,
    pub dirty: bool,
    pub version: u64,
}

impl GridMap {
    pub fn in_bounds(&self, coord: TileCoord) -> bool {
        coord.x >= 0 && coord.x < self.width && coord.z >= 0 && coord.z < self.height
    }

    pub fn idx(&self, coord: TileCoord) -> Option<usize> {
        if self.in_bounds(coord) {
            Some((coord.z * self.width + coord.x) as usize)
        } else {
            None
        }
    }

    pub fn get(&self, coord: TileCoord) -> Option<TileKind> {
        self.idx(coord).map(|i| self.tiles[i])
    }

    pub fn set(&mut self, coord: TileCoord, kind: TileKind) {
        if let Some(i) = self.idx(coord) {
            self.tiles[i] = kind;
            self.dirty = true;
            self.version = self.version.wrapping_add(1);
        }
    }

    pub fn reclaim_remaining(&self, coord: TileCoord) -> f32 {
        self.idx(coord)
            .map(|i| self.reclaim_timers[i])
            .unwrap_or(0.0)
    }

    pub fn set_reclaim_timer(&mut self, coord: TileCoord, secs: f32) {
        if let Some(i) = self.idx(coord) {
            self.reclaim_timers[i] = secs;
        }
    }

    /// World center of tile `(x, z)` centered around world origin `(0, 0, 0)`.
    pub fn tile_to_world(&self, coord: TileCoord) -> Vec3 {
        let half_w = self.width as f32 * 0.5;
        let half_h = self.height as f32 * 0.5;
        Vec3::new(
            (coord.x as f32 + 0.5 - half_w) * TILE_SIZE,
            0.0,
            (coord.z as f32 + 0.5 - half_h) * TILE_SIZE,
        )
    }

    pub fn world_to_tile(&self, world: Vec3) -> Option<TileCoord> {
        let half_w = self.width as f32 * 0.5;
        let half_h = self.height as f32 * 0.5;
        let x = (world.x / TILE_SIZE + half_w).floor() as i32;
        let z = (world.z / TILE_SIZE + half_h).floor() as i32;
        let coord = TileCoord::new(x, z);
        if self.in_bounds(coord) {
            Some(coord)
        } else {
            None
        }
    }

    pub fn zone_center_world(&self, tiles: &[TileCoord]) -> Vec3 {
        if tiles.is_empty() {
            return Vec3::ZERO;
        }
        let sum = tiles
            .iter()
            .fold(Vec3::ZERO, |acc, &c| acc + self.tile_to_world(c));
        sum / tiles.len() as f32
    }

    pub fn stage_count(&self) -> usize {
        self.checkpoints.len() + 1
    }

    pub fn stage_target_tiles(&self, stage: usize) -> &[TileCoord] {
        if stage < self.checkpoints.len() {
            &self.checkpoints[stage]
        } else {
            &self.goals
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FlowStage {
    pub dist: Vec<u16>,
    pub dir: Vec<Vec2>,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct FlowField {
    pub width: i32,
    pub height: i32,
    pub stages: Vec<FlowStage>,
    pub base_ground_path: Vec<Vec3>,
    pub air_path: Vec<Vec3>,
    pub hover_tile: Option<TileCoord>,
    pub hover_valid: bool,
    pub hover_block_reason: Option<BuildBlockReason>,
    pub hover_ground_path: Vec<Vec3>,
    pub base_path_len: usize,
    pub hover_path_len: usize,
    pub show_path_overlay: bool,
}

#[derive(Resource, Debug, Clone)]
pub struct EconomyState {
    pub gold: u32,
    pub gold_frac: f32,
    pub special_currency: u32,
    pub total_kills: u32,
    pub normal_kill_accum: u32,
    pub last_interest: u32,
    pub last_wave_bonus: u32,
    pub last_early_bonus: u32,
    pub interest_rate_bonus: f32,
    pub interest_cap_bonus: u32,
    pub passive_gold_bonus: f32,
    pub score: u64,
    pub match_elapsed_secs: f32,
    pub status_banner: Option<(String, f32)>,
}

impl Default for EconomyState {
    fn default() -> Self {
        Self {
            gold: 120,
            gold_frac: 0.0,
            special_currency: 1,
            total_kills: 0,
            normal_kill_accum: 0,
            last_interest: 0,
            last_wave_bonus: 0,
            last_early_bonus: 0,
            interest_rate_bonus: 0.0,
            interest_cap_bonus: 0,
            passive_gold_bonus: 0.0,
            score: 0,
            match_elapsed_secs: 0.0,
            status_banner: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PendingSpawn {
    pub archetype: EnemyArchetypeId,
    pub delay_remaining: f32,
    pub spawn_index: usize,
    pub hp_override: Option<f32>,
    pub bounty_override: Option<u32>,
    pub wave_number: u32,
}

#[derive(Resource, Debug, Clone)]
pub struct WaveState {
    pub current_wave: u32,
    pub max_standard_waves: u32,
    pub phase: WavePhase,
    pub prep_timer: f32,
    pub active_slots: f32,
    pub max_slots: u32,
    pub grace_timer: f32,
    pub grace_active: bool,
    pub spawn_queue: Vec<PendingSpawn>,
    pub wave_spawn_done: bool,
    pub active_boss: Option<Entity>,
    pub next_wave_has_air: bool,
    pub next_wave_title: String,
    pub next_wave_is_boss: bool,
    pub leaked_total: u32,
}

impl Default for WaveState {
    fn default() -> Self {
        Self {
            current_wave: 0,
            max_standard_waves: 30,
            phase: WavePhase::BuildPrep,
            prep_timer: 25.0,
            active_slots: 0.0,
            max_slots: 45,
            grace_timer: 10.0,
            grace_active: false,
            spawn_queue: Vec::new(),
            wave_spawn_done: true,
            active_boss: None,
            next_wave_has_air: false,
            next_wave_title: "Welle 1: Goblin-Plünderer".to_string(),
            next_wave_is_boss: false,
            leaked_total: 0,
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct SelectionState {
    pub selected_towers: Vec<Entity>,
    pub selected_enemy: Option<Entity>,
    pub build_tower_id: Option<String>,
    pub hovered_tile: Option<TileCoord>,
    pub show_all_hp_bars: bool,
    pub ui_pointer_captured: bool,
    pub last_click_time: f32,
    pub last_clicked_tower: Option<Entity>,
    pub hovered_tooltip: Option<String>,
}

#[derive(Resource, Debug, Clone)]
pub struct CameraRig {
    pub focus: Vec3,
    pub target_focus: Vec3,
    pub distance: f32,
    pub target_distance: f32,
    pub pitch_rad: f32,
    pub yaw_rad: f32,
    pub target_yaw: f32,
    pub bounds_min: Vec2,
    pub bounds_max: Vec2,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            target_focus: Vec3::ZERO,
            distance: 25.0,
            target_distance: 25.0,
            pitch_rad: 52.0_f32.to_radians(),
            yaw_rad: 0.0,
            target_yaw: 0.0,
            bounds_min: Vec2::new(-16.0, -12.0),
            bounds_max: Vec2::new(16.0, 12.0),
        }
    }
}

#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct GameSettings {
    pub master_volume: f32,
    pub sfx_volume: f32,
    pub muted: bool,
    pub edge_scrolling: bool,
    pub show_popups: bool,
    pub always_show_hp_bars: bool,
    pub camera_pan_speed: f32,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            sfx_volume: 0.85,
            muted: false,
            edge_scrolling: false,
            show_popups: true,
            always_show_hp_bars: false,
            camera_pan_speed: 18.0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreRecord {
    pub best_wave: u32,
    pub best_score: u64,
    pub victories: u32,
    pub total_kills: u32,
    pub fastest_clear_secs: Option<f32>,
}

#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileScores {
    pub records: HashMap<(MapId, FactionId, Difficulty), ScoreRecord>,
}

/// Drives the automated `--smoke-test` verification harness.
#[derive(Resource, Debug, Clone)]
pub struct SmokeTestState {
    pub enabled: bool,
    pub elapsed: f32,
    pub stage_01_maze_built: bool,
    pub stage_01_shot_taken: bool,
    pub stage_02_combat_built: bool,
    pub stage_02_shot_taken: bool,
    pub stage_03_ui_selected: bool,
    pub stage_03_shot_taken: bool,
    pub post_shot_buffer_frames: u32,
    pub exit_requested: bool,
}

impl Default for SmokeTestState {
    fn default() -> Self {
        Self {
            enabled: false,
            elapsed: 0.0,
            stage_01_maze_built: false,
            stage_01_shot_taken: false,
            stage_02_combat_built: false,
            stage_02_shot_taken: false,
            stage_03_ui_selected: false,
            stage_03_shot_taken: false,
            post_shot_buffer_frames: 0,
            exit_requested: false,
        }
    }
}

// ============================================================================
// ECS Components
// ============================================================================

/// Marker attached to every entity created inside an active match for clean reset/teardown.
#[derive(Component, Debug, Default)]
pub struct GameplayEntity;

#[derive(Component, Debug)]
pub struct MainCamera;

#[derive(Component, Debug)]
pub struct MapTileEntity {
    pub coord: TileCoord,
}

#[derive(Component, Debug)]
pub struct Tower {
    pub id: String,
    pub faction: FactionId,
    pub coord: TileCoord,
    pub tier: u8,
    pub invested_gold: u32,
    pub invested_special: u32,
    pub built_in_prep_wave: Option<u32>,
    pub priority: TargetPriority,
    pub cooldown: f32,
    pub ramp_stacks: u8,
    pub shot_counter: u8,
    pub kills: u32,
    pub aura_kill_counter: u32,
    pub total_damage: f32,
    pub dps_estimate: f32,
    pub target: Option<Entity>,
    pub recoil: f32,
    pub visual_spawned: bool,
}

#[derive(Component, Debug)]
pub struct TowerTurretHead {
    pub tower: Entity,
    pub base_offset: Vec3,
}

#[derive(Component, Debug)]
pub struct TowerVisualAccent {
    pub tower: Entity,
    pub spin_speed: f32,
    pub bob_amp: f32,
    pub base_y: f32,
}

#[derive(Component, Debug)]
pub struct Enemy {
    pub archetype: EnemyArchetypeId,
    pub domain: MovementDomain,
    pub hp: f32,
    pub max_hp: f32,
    pub base_speed: f32,
    pub base_armor: f32,
    pub slot_weight: f32,
    pub bounty: u32,
    pub bounty_paid: bool,
    pub is_boss: bool,
    pub is_elite: bool,
    pub current_stage: usize,
    pub last_stage_dist: u16,
    pub reroute_count: u8,
    pub loop_count: u32,
    pub regen_pct_per_sec: f32,
    pub split_on_death: Option<(EnemyArchetypeId, u8, f32)>,
    pub ability: BossAbilityKind,
    pub ability_cooldown: f32,
    pub shield_timer: f32,
    pub anim_phase: f32,
    pub visual_spawned: bool,
    pub scale: f32,
}

#[derive(Component, Debug, Default, Clone)]
pub struct StatusEffects {
    pub burn_dps: f32,
    pub burn_timer: f32,
    pub burn_anti_heal: f32,
    pub burn_spread: bool,
    pub burn_source: Option<Entity>,

    pub poison_dps: f32,
    pub poison_stacks: u8,
    pub poison_timer: f32,
    pub poison_anti_heal: f32,
    pub poison_puddle: bool,
    pub poison_source: Option<Entity>,

    pub slow_pct: f32,
    pub slow_timer: f32,
    pub frost_stacks: u8,
    pub freeze_timer: f32,
    pub freeze_immunity: f32,
    pub freeze_rewarded: bool,

    pub armor_shred: f32,
    pub armor_shred_timer: f32,

    pub stun_timer: f32,
    pub stun_immunity: f32,

    pub curse_amp: f32,
    pub curse_timer: f32,

    pub enrage_timer: f32,
    pub enrage_cooldown: f32,
}

impl StatusEffects {
    pub fn is_burning(&self) -> bool {
        self.burn_timer > 0.0
    }

    pub fn is_poisoned(&self) -> bool {
        self.poison_timer > 0.0
    }

    pub fn is_frozen(&self) -> bool {
        self.freeze_timer > 0.0
    }

    pub fn is_slowed_or_frozen(&self) -> bool {
        self.slow_timer > 0.0 || self.freeze_timer > 0.0
    }

    pub fn is_stunned(&self, is_boss: bool) -> bool {
        if is_boss {
            false
        } else {
            self.freeze_timer > 0.0 || self.stun_timer > 0.0
        }
    }

    pub fn anti_heal_fraction(&self) -> f32 {
        let mut ah: f32 = 0.0;
        if self.burn_timer > 0.0 {
            ah = ah.max(self.burn_anti_heal);
        }
        if self.poison_timer > 0.0 {
            ah = ah.max(self.poison_anti_heal);
        }
        ah.clamp(0.0, 0.95)
    }

    pub fn speed_multiplier(&self, is_boss: bool, max_slow_normal: f32, max_slow_boss: f32) -> f32 {
        if self.is_stunned(is_boss) {
            return 0.0;
        }
        let mut slow = if self.slow_timer > 0.0 {
            self.slow_pct
        } else {
            0.0
        };
        if is_boss && self.freeze_timer > 0.0 {
            slow += 0.20;
        }
        let max_slow = if is_boss {
            max_slow_boss
        } else {
            max_slow_normal
        };
        let slow_clamped = slow.clamp(0.0, max_slow);
        let enrage_bonus = if self.enrage_timer > 0.0 { 1.40 } else { 1.0 };
        ((1.0 - slow_clamped) * enrage_bonus).max(0.20)
    }
}

#[derive(Component, Debug)]
pub struct Projectile {
    pub source_tower: Entity,
    pub faction: FactionId,
    pub target: Option<Entity>,
    pub target_pos: Vec3,
    pub speed: f32,
    pub damage: f32,
    pub splash_radius: f32,
    pub air_bonus_mult: f32,
    pub armor_pen_pct: f32,
    pub on_hit: Vec<OnHitEffect>,
    pub bounce_remaining: u8,
    pub hit_entities: SmallVec<[Entity; 4]>,
    pub color: Color,
    pub is_heavy: bool,
    pub lifetime: f32,
}

#[derive(Component, Debug)]
pub struct GroundHazard {
    pub pos: Vec3,
    pub radius: f32,
    pub dps: f32,
    pub anti_heal: f32,
    pub timer: f32,
    pub source_tower: Entity,
    pub faction: FactionId,
}

#[derive(Component, Debug)]
pub struct LightningArcVfx {
    pub start: Vec3,
    pub end: Vec3,
    pub color: Color,
    pub timer: f32,
    pub max_timer: f32,
}

#[derive(Component, Debug)]
pub struct ParticleVfx {
    pub velocity: Vec3,
    pub gravity: f32,
    pub timer: f32,
    pub max_timer: f32,
    pub start_scale: f32,
}

#[derive(Component, Debug)]
pub struct FloatingPopup {
    pub world_pos: Vec3,
    pub text: String,
    pub color: Color,
    pub timer: f32,
    pub max_timer: f32,
}

#[derive(Component, Debug)]
pub struct HealthBarRoot {
    pub enemy: Entity,
    pub y_offset: f32,
    pub width: f32,
}

#[derive(Component, Debug)]
pub struct HealthBarFill {
    pub enemy: Entity,
}

/// Marker added in `CombatSet`/`LeakEconomySet`; actual `despawn()` happens ONLY in `CleanupSet`.
#[derive(Component, Debug, Default)]
pub struct PendingDespawn;

// ============================================================================
// Bevy 0.20 Buffered Messages (`#[derive(Message)]`)
// ============================================================================

#[derive(Message, Debug, Clone)]
pub struct StartMatchRequest {
    pub map_id: MapId,
    pub faction_id: FactionId,
    pub difficulty: Difficulty,
}

#[derive(Message, Debug, Clone)]
pub struct BuildTowerRequest {
    pub coord: TileCoord,
    pub tower_id: String,
    pub faction: FactionId,
    /// If true (e.g., smoke test), bypasses gold/special-currency cost checks.
    pub free: bool,
}

#[derive(Message, Debug, Clone)]
pub struct UpgradeTowerRequest {
    pub tower_entity: Entity,
    pub target_tower_id: String,
    pub free: bool,
}

#[derive(Message, Debug, Clone)]
pub struct SellTowerRequest {
    pub tower_entity: Entity,
}

#[derive(Message, Debug, Clone, Default)]
pub struct SendWaveEarlyRequest;

#[derive(Message, Debug, Clone)]
pub struct DamageAppliedMsg {
    pub enemy: Entity,
    pub source_tower: Option<Entity>,
    pub amount: f32,
    pub is_crit: bool,
    pub is_shatter: bool,
    pub pos: Vec3,
}

#[derive(Message, Debug, Clone)]
pub struct EnemyKilledMsg {
    pub enemy: Entity,
    pub archetype: EnemyArchetypeId,
    pub pos: Vec3,
    pub bounty: u32,
    pub is_boss: bool,
    pub is_elite: bool,
    pub was_burning: bool,
    pub was_slowed_or_frozen: bool,
    pub source_tower: Option<Entity>,
}

#[derive(Message, Debug, Clone)]
pub struct EnemyLeakedMsg {
    pub enemy: Entity,
    pub pos: Vec3,
    pub loop_count: u32,
}

#[derive(Message, Debug, Clone)]
pub struct FreezeTriggeredMsg {
    pub enemy: Entity,
    pub pos: Vec3,
}

#[derive(Message, Debug, Clone)]
pub struct TowerBuiltMsg {
    pub entity: Entity,
    pub coord: TileCoord,
    pub tower_id: String,
    pub faction: FactionId,
}

#[derive(Message, Debug, Clone)]
pub struct TowerUpgradedMsg {
    pub entity: Entity,
    pub coord: TileCoord,
    pub old_id: String,
    pub new_id: String,
    pub faction: FactionId,
}

#[derive(Message, Debug, Clone)]
pub struct TowerSoldMsg {
    pub coord: TileCoord,
    pub refund_gold: u32,
}

#[derive(Message, Debug, Clone)]
pub struct WaveStartedMsg {
    pub wave: u32,
    pub is_boss_wave: bool,
    pub has_air: bool,
}

#[derive(Message, Debug, Clone)]
pub struct WaveCompletedMsg {
    pub wave: u32,
    pub wave_bonus: u32,
    pub interest: u32,
}

#[derive(Message, Debug, Clone)]
pub struct PlaySfxMsg {
    pub kind: SfxEventKind,
    pub volume_scale: f32,
}

#[derive(Message, Debug, Clone)]
pub struct SpawnVfxMsg {
    pub kind: VfxKind,
    pub pos: Vec3,
    pub target_pos: Option<Vec3>,
    pub color: Color,
    pub radius: f32,
}

#[derive(Message, Debug, Clone)]
pub struct SpawnPopupMsg {
    pub pos: Vec3,
    pub text: String,
    pub color: Color,
}

#[derive(Message, Debug, Clone, Default)]
pub struct ConfigReloadMsg;

#[derive(Message, Debug, Clone, Default)]
pub struct SavePersistenceMsg;

// ============================================================================
// Pure Combat Math Helper (Shared by Combat + Unit Tests)
// ============================================================================

/// WC3-inspired armor damage multiplier:
/// - `armor >= 0`: `100 / (100 + armor)`
/// - `armor < 0`: `2 - 100 / (100 - armor)`
pub fn wc3_armor_multiplier(effective_armor: f32) -> f32 {
    if effective_armor >= 0.0 {
        100.0 / (100.0 + effective_armor)
    } else {
        2.0 - (100.0 / (100.0 - effective_armor))
    }
}

// ============================================================================
// Default Fallback GameConfigs (Populated & Overridden by RON Loader in Worker 4)
// ============================================================================

impl Default for GameConfigs {
    fn default() -> Self {
        Self {
            balance: default_balance_config(),
            maps: default_maps(),
            factions: default_factions(),
            enemies: default_enemies(),
            waves: default_waves(),
            loaded_from_override: Vec::new(),
        }
    }
}

pub fn default_balance_config() -> BalanceConfig {
    let mut difficulties = HashMap::new();
    difficulties.insert(
        Difficulty::Normal,
        DifficultyConfig {
            start_gold: 120,
            hp_mult: 1.0,
            armor_add: 0.0,
            bounty_mult: 1.0,
            max_slots: 45,
            score_mult: 1.0,
        },
    );
    difficulties.insert(
        Difficulty::Hard,
        DifficultyConfig {
            start_gold: 100,
            hp_mult: 1.25,
            armor_add: 8.0,
            bounty_mult: 0.95,
            max_slots: 40,
            score_mult: 1.5,
        },
    );
    difficulties.insert(
        Difficulty::Nightmare,
        DifficultyConfig {
            start_gold: 90,
            hp_mult: 1.55,
            armor_add: 16.0,
            bounty_mult: 0.90,
            max_slots: 35,
            score_mult: 2.25,
        },
    );
    BalanceConfig {
        prep_time_first_wave_secs: 25.0,
        prep_time_secs: 18.0,
        passive_gold_per_sec: 2.0,
        wave_bonus_base: 25,
        wave_bonus_per_wave: 6,
        interest_rate: 0.07,
        interest_cap: 250,
        early_send_base_per_sec: 2.0,
        early_send_wave_scale: 0.35,
        sell_refund_same_prep: 1.0,
        sell_refund_prep: 0.90,
        sell_refund_active_wave: 0.70,
        reclaim_cooldown_secs: 1.5,
        anti_juggle_reroute_threshold: 3,
        anti_juggle_enrage_secs: 3.0,
        anti_juggle_speed_mult: 1.40,
        anti_juggle_cooldown_secs: 6.0,
        limit_grace_period_secs: 10.0,
        max_slow_normal: 0.65,
        max_slow_boss: 0.35,
        max_armor_shred: 45.0,
        difficulties,
    }
}

pub fn default_maps() -> HashMap<MapId, MapConfig> {
    let mut maps = HashMap::new();
    maps.insert(
        MapId::LordaeronBastion,
        MapConfig {
            id: MapId::LordaeronBastion,
            display_name: "Lordaeron-Bastion".to_string(),
            biome: BiomeKind::LordaeronGrass,
            width: 28,
            height: 20,
            spawns: vec![MapZone {
                origin: TileCoord::new(1, 1),
                width: 2,
                height: 2,
            }],
            checkpoints: vec![
                MapZone {
                    origin: TileCoord::new(24, 2),
                    width: 2,
                    height: 2,
                },
                MapZone {
                    origin: TileCoord::new(2, 9),
                    width: 2,
                    height: 2,
                },
                MapZone {
                    origin: TileCoord::new(24, 15),
                    width: 2,
                    height: 2,
                },
            ],
            goal: MapZone {
                origin: TileCoord::new(1, 17),
                width: 2,
                height: 2,
            },
            blocked_tiles: vec![
                TileCoord::new(13, 5),
                TileCoord::new(14, 5),
                TileCoord::new(13, 13),
                TileCoord::new(14, 13),
            ],
        },
    );
    maps.insert(
        MapId::WintermaulGlacier,
        MapConfig {
            id: MapId::WintermaulGlacier,
            display_name: "Wintermaul-Gletscher".to_string(),
            biome: BiomeKind::WintermaulIce,
            width: 30,
            height: 22,
            spawns: vec![
                MapZone {
                    origin: TileCoord::new(2, 1),
                    width: 2,
                    height: 2,
                },
                MapZone {
                    origin: TileCoord::new(26, 1),
                    width: 2,
                    height: 2,
                },
            ],
            checkpoints: vec![
                MapZone {
                    origin: TileCoord::new(14, 9),
                    width: 2,
                    height: 2,
                },
                MapZone {
                    origin: TileCoord::new(14, 16),
                    width: 2,
                    height: 2,
                },
            ],
            goal: MapZone {
                origin: TileCoord::new(14, 19),
                width: 2,
                height: 2,
            },
            blocked_tiles: vec![
                TileCoord::new(9, 6),
                TileCoord::new(20, 6),
                TileCoord::new(9, 13),
                TileCoord::new(20, 13),
            ],
        },
    );
    maps
}

fn tower_map(list: Vec<TowerConfig>) -> HashMap<String, TowerConfig> {
    list.into_iter().map(|t| (t.id.clone(), t)).collect()
}

pub fn default_factions() -> HashMap<FactionId, FactionConfig> {
    let mut factions = HashMap::new();

    // 1. Elementar-Zirkel
    factions.insert(
        FactionId::ElementalCircle,
        FactionConfig {
            id: FactionId::ElementalCircle,
            display_name: "Elementar-Zirkel".to_string(),
            currency_name: "Elementar-Essenzen".to_string(),
            starting_currency: 1,
            currency_rule: CurrencyRule::EveryNWavesAndBoss {
                every_waves: 3,
                wave_amount: 1,
                boss_amount: 1,
            },
            primary_color: (0.28, 0.68, 0.95),
            accent_color: (0.95, 0.82, 0.35),
            root_buildable_towers: vec!["arcane_pillar".to_string()],
            towers: tower_map(vec![
                TowerConfig {
                    id: "arcane_pillar".into(),
                    name: "Arkansplitter-Pfeiler".into(),
                    hotkey: "Q".into(),
                    description: "Basis-Mazing-Turm; verzweigt in Feuer, Wasser oder Blitz.".into(),
                    tier: 1,
                    gold_cost: 15,
                    special_cost: 0,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 11.0,
                    attack_speed: 1.1,
                    range: 3.8,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 14.0,
                        splash_radius: 0.0,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![],
                    upgrades_to: vec![
                        "fire_obelisk".into(),
                        "water_fountain".into(),
                        "lightning_coil".into(),
                    ],
                    mesh_recipe: TowerMeshRecipe::ArcanePillar,
                    projectile_color: (0.72, 0.86, 1.0),
                },
                TowerConfig {
                    id: "fire_obelisk".into(),
                    name: "Feuer-Obelisk".into(),
                    hotkey: "Q".into(),
                    description: "Splash + 8 DPS Burn + 50% Anti-Heal.".into(),
                    tier: 2,
                    gold_cost: 40,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 26.0,
                    attack_speed: 0.85,
                    range: 4.2,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 11.5,
                        splash_radius: 1.2,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![OnHitEffect::Burn {
                        dps: 8.0,
                        duration: 4.0,
                        anti_heal: 0.50,
                        spread_on_death: false,
                    }],
                    upgrades_to: vec![
                        "inferno_spire".into(),
                        "steam_condenser".into(),
                        "plasma_core".into(),
                    ],
                    mesh_recipe: TowerMeshRecipe::FireObelisk,
                    projectile_color: (1.0, 0.42, 0.12),
                },
                TowerConfig {
                    id: "water_fountain".into(),
                    name: "Wasser-Brunnen".into(),
                    hotkey: "W".into(),
                    description: "Frost-Woge mit 30% Slow (3s).".into(),
                    tier: 2,
                    gold_cost: 35,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 16.0,
                    attack_speed: 1.1,
                    range: 4.4,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 13.0,
                        splash_radius: 0.8,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![OnHitEffect::Slow {
                        percent: 0.30,
                        duration: 3.0,
                        frost_stacks: 1,
                    }],
                    upgrades_to: vec![
                        "glacier_source".into(),
                        "steam_condenser".into(),
                        "storm_pillar".into(),
                    ],
                    mesh_recipe: TowerMeshRecipe::WaterFountain,
                    projectile_color: (0.25, 0.78, 1.0),
                },
                TowerConfig {
                    id: "lightning_coil".into(),
                    name: "Blitz-Spule".into(),
                    hotkey: "E".into(),
                    description: "Kettenblitz (3 Ziele) +25% vs. Luft.".into(),
                    tier: 2,
                    gold_cost: 45,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 24.0,
                    attack_speed: 1.0,
                    range: 4.6,
                    air_bonus_mult: 0.25,
                    attack_kind: AttackKind::ChainLightning {
                        bounces: 3,
                        decay_per_bounce: 0.15,
                        jump_range: 2.6,
                    },
                    on_hit_effects: vec![],
                    upgrades_to: vec![
                        "thunder_pylon".into(),
                        "plasma_core".into(),
                        "storm_pillar".into(),
                    ],
                    mesh_recipe: TowerMeshRecipe::LightningCoil,
                    projectile_color: (0.68, 0.92, 1.0),
                },
                TowerConfig {
                    id: "inferno_spire".into(),
                    name: "Inferno-Spire".into(),
                    hotkey: "Q".into(),
                    description: "Rein-Feuer T3: Großer Splash, 24 DPS Burn, 70% Anti-Heal.".into(),
                    tier: 3,
                    gold_cost: 110,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 72.0,
                    attack_speed: 0.8,
                    range: 4.6,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 12.0,
                        splash_radius: 1.7,
                        is_heavy: true,
                    },
                    on_hit_effects: vec![OnHitEffect::Burn {
                        dps: 24.0,
                        duration: 5.0,
                        anti_heal: 0.70,
                        spread_on_death: false,
                    }],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::InfernoSpire,
                    projectile_color: (1.0, 0.25, 0.05),
                },
                TowerConfig {
                    id: "glacier_source".into(),
                    name: "Gletscher-Quelle".into(),
                    hotkey: "Q".into(),
                    description: "Rein-Wasser T3: AoE 48% Slow + 20% Freeze-Chance.".into(),
                    tier: 3,
                    gold_cost: 95,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 38.0,
                    attack_speed: 1.1,
                    range: 5.0,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 13.0,
                        splash_radius: 1.5,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![
                        OnHitEffect::Slow {
                            percent: 0.48,
                            duration: 3.5,
                            frost_stacks: 2,
                        },
                        OnHitEffect::FreezeChance {
                            chance: 0.20,
                            duration: 0.8,
                        },
                    ],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::GlacierSource,
                    projectile_color: (0.45, 0.92, 1.0),
                },
                TowerConfig {
                    id: "thunder_pylon".into(),
                    name: "Gewitter-Pylon".into(),
                    hotkey: "Q".into(),
                    description: "Rein-Blitz T3: 6-Ziel-Kettenblitz, +35% Luft, 15% Crit.".into(),
                    tier: 3,
                    gold_cost: 120,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 62.0,
                    attack_speed: 1.15,
                    range: 5.2,
                    air_bonus_mult: 0.35,
                    attack_kind: AttackKind::ChainLightning {
                        bounces: 6,
                        decay_per_bounce: 0.12,
                        jump_range: 2.8,
                    },
                    on_hit_effects: vec![OnHitEffect::Crit {
                        chance: 0.15,
                        mult: 1.8,
                    }],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::ThunderPylon,
                    projectile_color: (0.85, 0.95, 1.0),
                },
                TowerConfig {
                    id: "steam_condenser".into(),
                    name: "Dampf-Kondensator".into(),
                    hotkey: "W".into(),
                    description: "Dual (Feuer+Wasser): Dampf-AoE mit -25% Slow & -25 Armor Shred."
                        .into(),
                    tier: 3,
                    gold_cost: 105,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundOnly,
                    damage: 48.0,
                    attack_speed: 0.9,
                    range: 4.2,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::ConeAoe { radius: 1.9 },
                    on_hit_effects: vec![
                        OnHitEffect::Slow {
                            percent: 0.25,
                            duration: 3.5,
                            frost_stacks: 0,
                        },
                        OnHitEffect::ArmorShred {
                            amount: 25.0,
                            max_stack: 25.0,
                            duration: 4.0,
                        },
                    ],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::SteamCondenser,
                    projectile_color: (0.85, 0.90, 0.94),
                },
                TowerConfig {
                    id: "plasma_core".into(),
                    name: "Plasma-Kern".into(),
                    hotkey: "E".into(),
                    description: "Dual (Feuer+Blitz): 25% Crit (2x) + 20% Max-HP Tod-Explosion."
                        .into(),
                    tier: 3,
                    gold_cost: 125,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 115.0,
                    attack_speed: 0.95,
                    range: 5.0,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 18.0,
                        splash_radius: 0.0,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![
                        OnHitEffect::Crit {
                            chance: 0.25,
                            mult: 2.0,
                        },
                        OnHitEffect::DeathExplosion {
                            max_hp_pct: 0.20,
                            radius: 1.8,
                        },
                    ],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::PlasmaCore,
                    projectile_color: (0.98, 0.35, 0.95),
                },
                TowerConfig {
                    id: "storm_pillar".into(),
                    name: "Sturm-Säule".into(),
                    hotkey: "E".into(),
                    description: "Dual (Wasser+Blitz): 5-Ziel-Kette, Mini-Stun, Slow, +45% Luft."
                        .into(),
                    tier: 3,
                    gold_cost: 115,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 54.0,
                    attack_speed: 1.05,
                    range: 5.2,
                    air_bonus_mult: 0.45,
                    attack_kind: AttackKind::ChainLightning {
                        bounces: 5,
                        decay_per_bounce: 0.12,
                        jump_range: 2.8,
                    },
                    on_hit_effects: vec![
                        OnHitEffect::Slow {
                            percent: 0.20,
                            duration: 2.5,
                            frost_stacks: 1,
                        },
                        OnHitEffect::MiniStun {
                            duration: 0.35,
                            immunity: 2.5,
                        },
                    ],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::StormPillar,
                    projectile_color: (0.35, 0.95, 0.98),
                },
            ]),
        },
    );

    // 2. Zitadelle (Menschen-Ingenieure)
    factions.insert(
        FactionId::Citadel,
        FactionConfig {
            id: FactionId::Citadel,
            display_name: "Zitadelle".to_string(),
            currency_name: "Handwerkskerne".to_string(),
            starting_currency: 1,
            currency_rule: CurrencyRule::EveryNWavesAndTreasury {
                every_waves: 2,
                wave_amount: 1,
            },
            primary_color: (0.74, 0.72, 0.68),
            accent_color: (0.92, 0.75, 0.24),
            root_buildable_towers: vec![
                "watchtower".into(),
                "flak_hunter".into(),
                "guild_treasury".into(),
            ],
            towers: tower_map(vec![
                TowerConfig {
                    id: "watchtower".into(),
                    name: "Wachturm".into(),
                    hotkey: "Q".into(),
                    description: "Klassischer Pfeilturm; verzweigt in 3 Tech-Pfade.".into(),
                    tier: 1,
                    gold_cost: 15,
                    special_cost: 0,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 12.0,
                    attack_speed: 1.15,
                    range: 4.2,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 16.0,
                        splash_radius: 0.0,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![],
                    upgrades_to: vec![
                        "rapid_fire_tower".into(),
                        "multishot_tower".into(),
                        "heavy_cannon".into(),
                    ],
                    mesh_recipe: TowerMeshRecipe::Watchtower,
                    projectile_color: (0.95, 0.85, 0.55),
                },
                TowerConfig {
                    id: "rapid_fire_tower".into(),
                    name: "Schnellfeuer-Turm".into(),
                    hotkey: "Q".into(),
                    description: "3.2 Schuss/s; jeder 4. Treffer ignoriert 50% Rüstung.".into(),
                    tier: 2,
                    gold_cost: 45,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 11.0,
                    attack_speed: 3.2,
                    range: 4.4,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 19.0,
                        splash_radius: 0.0,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![OnHitEffect::ArmorPen {
                        every_nth: 4,
                        ignore_pct: 0.50,
                    }],
                    upgrades_to: vec!["gatling_ballista".into()],
                    mesh_recipe: TowerMeshRecipe::RapidFireTower,
                    projectile_color: (1.0, 0.88, 0.35),
                },
                TowerConfig {
                    id: "gatling_ballista".into(),
                    name: "Gatling-Balliste".into(),
                    hotkey: "Q".into(),
                    description: "4.8-6.0 Schuss/s Ramp-Up; zerlegt Einzelziele & Bosse.".into(),
                    tier: 3,
                    gold_cost: 115,
                    special_cost: 2,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 22.0,
                    attack_speed: 4.8,
                    range: 4.8,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 22.0,
                        splash_radius: 0.0,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![
                        OnHitEffect::ArmorPen {
                            every_nth: 4,
                            ignore_pct: 0.50,
                        },
                        OnHitEffect::RampAttackSpeed {
                            max_bonus_aspd: 1.2,
                        },
                    ],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::GatlingBallista,
                    projectile_color: (1.0, 0.72, 0.22),
                },
                TowerConfig {
                    id: "multishot_tower".into(),
                    name: "Mehrfachschuss-Turm".into(),
                    hotkey: "W".into(),
                    description: "Feuert 3 Pfeile gleichzeitig auf verschiedene Ziele.".into(),
                    tier: 2,
                    gold_cost: 50,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 18.0,
                    attack_speed: 1.2,
                    range: 4.6,
                    air_bonus_mult: 0.15,
                    attack_kind: AttackKind::MultiShot {
                        count: 3,
                        speed: 17.0,
                        bounce_once: false,
                    },
                    on_hit_effects: vec![],
                    upgrades_to: vec!["volley_bastion".into()],
                    mesh_recipe: TowerMeshRecipe::MultishotTower,
                    projectile_color: (0.95, 0.90, 0.65),
                },
                TowerConfig {
                    id: "volley_bastion".into(),
                    name: "Salven-Bastion".into(),
                    hotkey: "Q".into(),
                    description: "5 Pfeile gleichzeitig, jeder prallt 1x ab.".into(),
                    tier: 3,
                    gold_cost: 125,
                    special_cost: 2,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 36.0,
                    attack_speed: 1.35,
                    range: 5.2,
                    air_bonus_mult: 0.20,
                    attack_kind: AttackKind::MultiShot {
                        count: 5,
                        speed: 18.0,
                        bounce_once: true,
                    },
                    on_hit_effects: vec![],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::VolleyBastion,
                    projectile_color: (1.0, 0.92, 0.55),
                },
                TowerConfig {
                    id: "heavy_cannon".into(),
                    name: "Schwere Kanone".into(),
                    hotkey: "E".into(),
                    description: "Boden-Splash (1.5) + -15 Armor Shred.".into(),
                    tier: 2,
                    gold_cost: 55,
                    special_cost: 1,
                    max_count: None,
                    target_mask: TargetMask::GroundOnly,
                    damage: 52.0,
                    attack_speed: 0.6,
                    range: 4.8,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 10.5,
                        splash_radius: 1.5,
                        is_heavy: true,
                    },
                    on_hit_effects: vec![OnHitEffect::ArmorShred {
                        amount: 15.0,
                        max_stack: 15.0,
                        duration: 3.0,
                    }],
                    upgrades_to: vec!["siege_mortar".into()],
                    mesh_recipe: TowerMeshRecipe::HeavyCannon,
                    projectile_color: (0.35, 0.35, 0.38),
                },
                TowerConfig {
                    id: "siege_mortar".into(),
                    name: "Belagerungs-Mörser".into(),
                    hotkey: "Q".into(),
                    description: "Artillerie-Splash (2.1), -25 Armor Shred + Erschütterungs-Slow."
                        .into(),
                    tier: 3,
                    gold_cost: 135,
                    special_cost: 2,
                    max_count: None,
                    target_mask: TargetMask::GroundOnly,
                    damage: 145.0,
                    attack_speed: 0.55,
                    range: 6.0,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 9.5,
                        splash_radius: 2.1,
                        is_heavy: true,
                    },
                    on_hit_effects: vec![
                        OnHitEffect::ArmorShred {
                            amount: 25.0,
                            max_stack: 25.0,
                            duration: 4.0,
                        },
                        OnHitEffect::Slow {
                            percent: 0.25,
                            duration: 1.2,
                            frost_stacks: 0,
                        },
                    ],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::SiegeMortar,
                    projectile_color: (0.95, 0.50, 0.15),
                },
                TowerConfig {
                    id: "flak_hunter".into(),
                    name: "Flak-Harpyienjäger".into(),
                    hotkey: "W".into(),
                    description: "Reine Luftabwehr (AirOnly) mit 1.4 Air-Splash & 6.5 Reichweite."
                        .into(),
                    tier: 2,
                    gold_cost: 50,
                    special_cost: 0,
                    max_count: None,
                    target_mask: TargetMask::AirOnly,
                    damage: 42.0,
                    attack_speed: 1.4,
                    range: 6.5,
                    air_bonus_mult: 0.25,
                    attack_kind: AttackKind::Projectile {
                        speed: 20.0,
                        splash_radius: 1.4,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::FlakHunter,
                    projectile_color: (1.0, 0.80, 0.20),
                },
                TowerConfig {
                    id: "guild_treasury".into(),
                    name: "Gilden-Schatzkammer".into(),
                    hotkey: "E".into(),
                    description: "+3g/s Passiv, +1% Zins (+40g Cap), +1 Kern alle 4 Wellen (max 3)."
                        .into(),
                    tier: 2,
                    gold_cost: 80,
                    special_cost: 0,
                    max_count: Some(3),
                    target_mask: TargetMask::GroundOnly,
                    damage: 0.0,
                    attack_speed: 0.0,
                    range: 2.0,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::SupportAura {
                        aura_radius: 2.0,
                        dmg_bonus: 0.0,
                        bounty_bonus: 0.0,
                        soul_mult: 1.0,
                        passive_gold: 3.0,
                        interest_bonus: 0.01,
                        interest_cap_bonus: 40,
                        core_every_waves: 4,
                    },
                    on_hit_effects: vec![],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::GuildTreasury,
                    projectile_color: (1.0, 0.85, 0.25),
                },
            ]),
        },
    );

    // 3. Drachenklan
    factions.insert(
        FactionId::DragonClan,
        FactionConfig {
            id: FactionId::DragonClan,
            display_name: "Drachenklan".to_string(),
            currency_name: "Glutseelen".to_string(),
            starting_currency: 4,
            currency_rule: CurrencyRule::BurnKillsAndBoss {
                burn_kill_amount: 1,
                normal_kills_per_unit: 5,
                boss_amount: 15,
            },
            primary_color: (0.72, 0.16, 0.12),
            accent_color: (1.0, 0.55, 0.15),
            root_buildable_towers: vec!["brood_nest".into(), "gold_hoard_shrine".into()],
            towers: tower_map(vec![
                TowerConfig {
                    id: "brood_nest".into(),
                    name: "Drachenbrut-Nest".into(),
                    hotkey: "Q".into(),
                    description: "Basis-Drache; entzündet Gegner für frühe Glutseelen.".into(),
                    tier: 1,
                    gold_cost: 20,
                    special_cost: 0,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 15.0,
                    attack_speed: 1.0,
                    range: 3.8,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 13.5,
                        splash_radius: 0.6,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![OnHitEffect::Burn {
                        dps: 4.0,
                        duration: 3.0,
                        anti_heal: 0.35,
                        spread_on_death: false,
                    }],
                    upgrades_to: vec![
                        "red_dragon_roost".into(),
                        "bronze_dragon_spire".into(),
                        "storm_dragon_perch".into(),
                    ],
                    mesh_recipe: TowerMeshRecipe::BroodNest,
                    projectile_color: (1.0, 0.45, 0.12),
                },
                TowerConfig {
                    id: "red_dragon_roost".into(),
                    name: "Rotdrachen-Hort".into(),
                    hotkey: "Q".into(),
                    description: "Flammenkegel-AoE + 16 DPS Burn + 60% Anti-Heal.".into(),
                    tier: 2,
                    gold_cost: 65,
                    special_cost: 8,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 38.0,
                    attack_speed: 0.85,
                    range: 4.4,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::ConeAoe { radius: 1.5 },
                    on_hit_effects: vec![OnHitEffect::Burn {
                        dps: 16.0,
                        duration: 4.5,
                        anti_heal: 0.60,
                        spread_on_death: false,
                    }],
                    upgrades_to: vec!["elder_fire_dragon".into()],
                    mesh_recipe: TowerMeshRecipe::RedDragonRoost,
                    projectile_color: (1.0, 0.30, 0.08),
                },
                TowerConfig {
                    id: "elder_fire_dragon".into(),
                    name: "Ältester Feuerdrache".into(),
                    hotkey: "Q".into(),
                    description: "Inferno-Atem (2.0), 40 DPS Burn, 80% Anti-Heal, springt bei Tod."
                        .into(),
                    tier: 3,
                    gold_cost: 170,
                    special_cost: 25,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 110.0,
                    attack_speed: 0.9,
                    range: 5.0,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::ConeAoe { radius: 2.0 },
                    on_hit_effects: vec![OnHitEffect::Burn {
                        dps: 40.0,
                        duration: 5.0,
                        anti_heal: 0.80,
                        spread_on_death: true,
                    }],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::ElderFireDragon,
                    projectile_color: (1.0, 0.22, 0.04),
                },
                TowerConfig {
                    id: "bronze_dragon_spire".into(),
                    name: "Bronzedrachen-Spitze".into(),
                    hotkey: "W".into(),
                    description: "Sand-Atem: -30 Armor Shred (5s) + 20% Slow.".into(),
                    tier: 2,
                    gold_cost: 70,
                    special_cost: 10,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 44.0,
                    attack_speed: 0.95,
                    range: 4.6,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 14.0,
                        splash_radius: 1.0,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![
                        OnHitEffect::ArmorShred {
                            amount: 30.0,
                            max_stack: 30.0,
                            duration: 5.0,
                        },
                        OnHitEffect::Slow {
                            percent: 0.20,
                            duration: 3.0,
                            frost_stacks: 0,
                        },
                    ],
                    upgrades_to: vec!["time_dragon_sanctum".into()],
                    mesh_recipe: TowerMeshRecipe::BronzeDragonSpire,
                    projectile_color: (0.90, 0.72, 0.32),
                },
                TowerConfig {
                    id: "time_dragon_sanctum".into(),
                    name: "Zeitdrachen-Sanctum".into(),
                    hotkey: "Q".into(),
                    description: "-40 Armor Shred + 25% Slow-Aura (Radius 3.5).".into(),
                    tier: 3,
                    gold_cost: 165,
                    special_cost: 25,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 95.0,
                    attack_speed: 1.1,
                    range: 5.2,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 15.0,
                        splash_radius: 1.2,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![
                        OnHitEffect::ArmorShred {
                            amount: 40.0,
                            max_stack: 40.0,
                            duration: 5.0,
                        },
                        OnHitEffect::SlowAura {
                            radius: 3.5,
                            percent: 0.25,
                        },
                    ],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::TimeDragonSanctum,
                    projectile_color: (0.95, 0.84, 0.45),
                },
                TowerConfig {
                    id: "storm_dragon_perch".into(),
                    name: "Sturmdrachen-Sitz".into(),
                    hotkey: "E".into(),
                    description: "Blitz-Atem (3 Ziele), +40% vs. Luft.".into(),
                    tier: 2,
                    gold_cost: 70,
                    special_cost: 10,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 36.0,
                    attack_speed: 1.25,
                    range: 5.2,
                    air_bonus_mult: 0.40,
                    attack_kind: AttackKind::ChainLightning {
                        bounces: 3,
                        decay_per_bounce: 0.14,
                        jump_range: 2.8,
                    },
                    on_hit_effects: vec![],
                    upgrades_to: vec!["sky_wyrm_peak".into()],
                    mesh_recipe: TowerMeshRecipe::StormDragonPerch,
                    projectile_color: (0.45, 0.85, 1.0),
                },
                TowerConfig {
                    id: "sky_wyrm_peak".into(),
                    name: "Himmelswyrm-Gipfel".into(),
                    hotkey: "Q".into(),
                    description: "5-Ziel-Sturm-Atem, +55% Luft, ignoriert 40% Rüstung.".into(),
                    tier: 3,
                    gold_cost: 180,
                    special_cost: 28,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 88.0,
                    attack_speed: 1.4,
                    range: 6.0,
                    air_bonus_mult: 0.55,
                    attack_kind: AttackKind::ChainLightning {
                        bounces: 5,
                        decay_per_bounce: 0.10,
                        jump_range: 3.0,
                    },
                    on_hit_effects: vec![OnHitEffect::ArmorPen {
                        every_nth: 1,
                        ignore_pct: 0.40,
                    }],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::SkyWyrmPeak,
                    projectile_color: (0.75, 0.96, 1.0),
                },
                TowerConfig {
                    id: "gold_hoard_shrine".into(),
                    name: "Goldhort-Schrein".into(),
                    hotkey: "W".into(),
                    description: "Aura (3.6): +15% Drachen-Schaden & +20% Kill-Bounty (max 3)."
                        .into(),
                    tier: 2,
                    gold_cost: 90,
                    special_cost: 12,
                    max_count: Some(3),
                    target_mask: TargetMask::GroundOnly,
                    damage: 0.0,
                    attack_speed: 0.0,
                    range: 3.6,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::SupportAura {
                        aura_radius: 3.6,
                        dmg_bonus: 0.15,
                        bounty_bonus: 0.20,
                        soul_mult: 1.0,
                        passive_gold: 0.0,
                        interest_bonus: 0.0,
                        interest_cap_bonus: 0,
                        core_every_waves: 0,
                    },
                    on_hit_effects: vec![],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::GoldHoardShrine,
                    projectile_color: (1.0, 0.85, 0.20),
                },
            ]),
        },
    );

    // 4. Frostwacht (Eismenschen)
    factions.insert(
        FactionId::Frostguard,
        FactionConfig {
            id: FactionId::Frostguard,
            display_name: "Frostwacht".to_string(),
            currency_name: "Frostsplitter".to_string(),
            starting_currency: 4,
            currency_rule: CurrencyRule::SlowedKillsAndFreeze {
                slowed_kill_amount: 1,
                freeze_proc_amount: 1,
            },
            primary_color: (0.35, 0.78, 0.98),
            accent_color: (0.85, 0.96, 1.0),
            root_buildable_towers: vec!["ice_pillar".into(), "northwind_tower".into()],
            towers: tower_map(vec![
                TowerConfig {
                    id: "ice_pillar".into(),
                    name: "Eis-Pfeiler".into(),
                    hotkey: "Q".into(),
                    description: "Basis-Eisturm: 18% Slow + 1 Frost-Stack.".into(),
                    tier: 1,
                    gold_cost: 15,
                    special_cost: 0,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 10.0,
                    attack_speed: 1.1,
                    range: 4.0,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 15.0,
                        splash_radius: 0.0,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![OnHitEffect::Slow {
                        percent: 0.18,
                        duration: 2.5,
                        frost_stacks: 1,
                    }],
                    upgrades_to: vec![
                        "frost_spear_thrower".into(),
                        "blizzard_obelisk".into(),
                        "ice_shard_catapult".into(),
                    ],
                    mesh_recipe: TowerMeshRecipe::IcePillar,
                    projectile_color: (0.62, 0.92, 1.0),
                },
                TowerConfig {
                    id: "frost_spear_thrower".into(),
                    name: "Frostspeer-Werfer".into(),
                    hotkey: "Q".into(),
                    description: "Shatter: +80% Schaden gegen gefrorene Ziele!".into(),
                    tier: 2,
                    gold_cost: 45,
                    special_cost: 5,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 34.0,
                    attack_speed: 1.0,
                    range: 5.0,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 19.0,
                        splash_radius: 0.0,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![OnHitEffect::Shatter {
                        bonus_mult: 0.80,
                        nova_radius: 0.0,
                    }],
                    upgrades_to: vec!["glacier_sniper".into()],
                    mesh_recipe: TowerMeshRecipe::FrostSpearThrower,
                    projectile_color: (0.50, 0.88, 1.0),
                },
                TowerConfig {
                    id: "glacier_sniper".into(),
                    name: "Gletscher-Scharfschütze".into(),
                    hotkey: "Q".into(),
                    description: "Shatter: +110% Schaden + 1.6 Frost-Nova bei Shatter-Kill.".into(),
                    tier: 3,
                    gold_cost: 125,
                    special_cost: 18,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 110.0,
                    attack_speed: 1.05,
                    range: 6.2,
                    air_bonus_mult: 0.15,
                    attack_kind: AttackKind::Projectile {
                        speed: 23.0,
                        splash_radius: 0.0,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![OnHitEffect::Shatter {
                        bonus_mult: 1.10,
                        nova_radius: 1.6,
                    }],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::GlacierSniper,
                    projectile_color: (0.80, 0.98, 1.0),
                },
                TowerConfig {
                    id: "blizzard_obelisk".into(),
                    name: "Schneesturm-Obelisk".into(),
                    hotkey: "W".into(),
                    description: "AoE-Kälte (1.6), 38% Slow + 2 Frost-Stacks (5 = Freeze!).".into(),
                    tier: 2,
                    gold_cost: 55,
                    special_cost: 6,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 18.0,
                    attack_speed: 1.1,
                    range: 4.2,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::ConeAoe { radius: 1.6 },
                    on_hit_effects: vec![OnHitEffect::Slow {
                        percent: 0.38,
                        duration: 3.0,
                        frost_stacks: 2,
                    }],
                    upgrades_to: vec!["blizzard_spire".into()],
                    mesh_recipe: TowerMeshRecipe::BlizzardObelisk,
                    projectile_color: (0.45, 0.85, 1.0),
                },
                TowerConfig {
                    id: "blizzard_spire".into(),
                    name: "Blizzard-Spire".into(),
                    hotkey: "Q".into(),
                    description: "Großer Blizzard (2.1), 55% Slow, 3 Stacks + -15 Armor Shred."
                        .into(),
                    tier: 3,
                    gold_cost: 140,
                    special_cost: 22,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 46.0,
                    attack_speed: 1.2,
                    range: 4.8,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::ConeAoe { radius: 2.1 },
                    on_hit_effects: vec![
                        OnHitEffect::Slow {
                            percent: 0.55,
                            duration: 3.5,
                            frost_stacks: 3,
                        },
                        OnHitEffect::ArmorShred {
                            amount: 15.0,
                            max_stack: 15.0,
                            duration: 3.5,
                        },
                    ],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::BlizzardSpire,
                    projectile_color: (0.70, 0.95, 1.0),
                },
                TowerConfig {
                    id: "ice_shard_catapult".into(),
                    name: "Eissplitter-Katapult".into(),
                    hotkey: "E".into(),
                    description: "Boden-Splash (1.8) zerberstet in Eisscherben vs. Schwärme."
                        .into(),
                    tier: 2,
                    gold_cost: 60,
                    special_cost: 8,
                    max_count: None,
                    target_mask: TargetMask::GroundOnly,
                    damage: 42.0,
                    attack_speed: 0.75,
                    range: 4.8,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 11.0,
                        splash_radius: 1.8,
                        is_heavy: true,
                    },
                    on_hit_effects: vec![OnHitEffect::Slow {
                        percent: 0.25,
                        duration: 2.5,
                        frost_stacks: 1,
                    }],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::IceShardCatapult,
                    projectile_color: (0.55, 0.90, 1.0),
                },
                TowerConfig {
                    id: "northwind_tower".into(),
                    name: "Nordwind-Turm".into(),
                    hotkey: "W".into(),
                    description: "AirOnly: 45% Air-Slow, +2 Frost-Stacks & Rückstoß.".into(),
                    tier: 2,
                    gold_cost: 50,
                    special_cost: 5,
                    max_count: None,
                    target_mask: TargetMask::AirOnly,
                    damage: 38.0,
                    attack_speed: 1.3,
                    range: 6.2,
                    air_bonus_mult: 0.30,
                    attack_kind: AttackKind::Projectile {
                        speed: 18.0,
                        splash_radius: 1.0,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![
                        OnHitEffect::Slow {
                            percent: 0.45,
                            duration: 3.0,
                            frost_stacks: 2,
                        },
                        OnHitEffect::AirKnockback { distance: 0.4 },
                    ],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::NorthwindTower,
                    projectile_color: (0.78, 0.97, 1.0),
                },
            ]),
        },
    );

    // 5. Nekropolis (Untote)
    factions.insert(
        FactionId::Necropolis,
        FactionConfig {
            id: FactionId::Necropolis,
            display_name: "Nekropolis".to_string(),
            currency_name: "Seelen".to_string(),
            starting_currency: 6,
            currency_rule: CurrencyRule::EveryKill {
                normal_amount: 1,
                elite_amount: 3,
                boss_amount: 20,
            },
            primary_color: (0.34, 0.24, 0.44),
            accent_color: (0.45, 0.96, 0.32),
            root_buildable_towers: vec!["bone_spire".into(), "soul_crypt".into()],
            towers: tower_map(vec![
                TowerConfig {
                    id: "bone_spire".into(),
                    name: "Knochen-Spire".into(),
                    hotkey: "Q".into(),
                    description: "Basis-Untotenturm; schneller Knochenbolzen.".into(),
                    tier: 1,
                    gold_cost: 15,
                    special_cost: 0,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 12.0,
                    attack_speed: 1.2,
                    range: 4.0,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 15.5,
                        splash_radius: 0.0,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![],
                    upgrades_to: vec![
                        "plague_ziggurat".into(),
                        "decay_obelisk".into(),
                        "banshee_tower".into(),
                    ],
                    mesh_recipe: TowerMeshRecipe::BoneSpire,
                    projectile_color: (0.85, 0.88, 0.78),
                },
                TowerConfig {
                    id: "plague_ziggurat".into(),
                    name: "Seuchen-Ziggurat".into(),
                    hotkey: "Q".into(),
                    description: "Giftwolke (1.6), 14 DPS Gift + 80% Anti-Heal vs. Vampire."
                        .into(),
                    tier: 2,
                    gold_cost: 50,
                    special_cost: 12,
                    max_count: None,
                    target_mask: TargetMask::GroundOnly,
                    damage: 24.0,
                    attack_speed: 0.9,
                    range: 4.2,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 11.5,
                        splash_radius: 1.6,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![OnHitEffect::Poison {
                        dps: 14.0,
                        duration: 5.0,
                        anti_heal: 0.80,
                        puddle_on_death: false,
                    }],
                    upgrades_to: vec!["pest_cathedral".into()],
                    mesh_recipe: TowerMeshRecipe::PlagueZiggurat,
                    projectile_color: (0.35, 0.95, 0.25),
                },
                TowerConfig {
                    id: "pest_cathedral".into(),
                    name: "Pest-Kathedrale".into(),
                    hotkey: "Q".into(),
                    description: "Seuche (2.1), 34 DPS Gift, 95% Anti-Heal + Giftpfütze bei Tod."
                        .into(),
                    tier: 3,
                    gold_cost: 130,
                    special_cost: 35,
                    max_count: None,
                    target_mask: TargetMask::GroundOnly,
                    damage: 62.0,
                    attack_speed: 1.0,
                    range: 4.8,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Projectile {
                        speed: 12.0,
                        splash_radius: 2.1,
                        is_heavy: true,
                    },
                    on_hit_effects: vec![OnHitEffect::Poison {
                        dps: 34.0,
                        duration: 5.5,
                        anti_heal: 0.95,
                        puddle_on_death: true,
                    }],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::PestCathedral,
                    projectile_color: (0.28, 1.0, 0.18),
                },
                TowerConfig {
                    id: "decay_obelisk".into(),
                    name: "Obelisk der Verwesung".into(),
                    hotkey: "W".into(),
                    description: "Nekro-Strahl: stapelt -8 Armor Shred bis -35.".into(),
                    tier: 2,
                    gold_cost: 55,
                    special_cost: 15,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 32.0,
                    attack_speed: 1.1,
                    range: 4.6,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::Beam {
                        ramp_armor_shred: 8.0,
                    },
                    on_hit_effects: vec![OnHitEffect::ArmorShred {
                        amount: 8.0,
                        max_stack: 35.0,
                        duration: 4.0,
                    }],
                    upgrades_to: vec!["lich_throne".into()],
                    mesh_recipe: TowerMeshRecipe::DecayObelisk,
                    projectile_color: (0.72, 0.30, 0.95),
                },
                TowerConfig {
                    id: "lich_throne".into(),
                    name: "Lich-Thron".into(),
                    hotkey: "Q".into(),
                    description: "Frost-Nekro-Nova (1.7): -45 Armor Shred + 35% Slow.".into(),
                    tier: 3,
                    gold_cost: 155,
                    special_cost: 45,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 96.0,
                    attack_speed: 1.1,
                    range: 5.4,
                    air_bonus_mult: 0.15,
                    attack_kind: AttackKind::Projectile {
                        speed: 16.0,
                        splash_radius: 1.7,
                        is_heavy: false,
                    },
                    on_hit_effects: vec![
                        OnHitEffect::ArmorShred {
                            amount: 45.0,
                            max_stack: 45.0,
                            duration: 5.0,
                        },
                        OnHitEffect::Slow {
                            percent: 0.35,
                            duration: 3.5,
                            frost_stacks: 2,
                        },
                    ],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::LichThron,
                    projectile_color: (0.60, 0.45, 1.0),
                },
                TowerConfig {
                    id: "banshee_tower".into(),
                    name: "Banshee-Turm".into(),
                    hotkey: "E".into(),
                    description: "2-Ziel-Schrei, +40% Luft + Fluch (+20% eingehender Schaden)."
                        .into(),
                    tier: 2,
                    gold_cost: 50,
                    special_cost: 12,
                    max_count: None,
                    target_mask: TargetMask::GroundAndAir,
                    damage: 28.0,
                    attack_speed: 1.3,
                    range: 5.4,
                    air_bonus_mult: 0.40,
                    attack_kind: AttackKind::MultiShot {
                        count: 2,
                        speed: 18.0,
                        bounce_once: false,
                    },
                    on_hit_effects: vec![OnHitEffect::Curse {
                        dmg_amp: 0.20,
                        duration: 4.0,
                    }],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::BansheeTower,
                    projectile_color: (0.82, 0.35, 1.0),
                },
                TowerConfig {
                    id: "soul_crypt".into(),
                    name: "Seelen-Krypta".into(),
                    hotkey: "W".into(),
                    description: "Verdoppelt Seelen im Radius 4.5 + +5g alle 8 Kills (max 3)."
                        .into(),
                    tier: 2,
                    gold_cost: 70,
                    special_cost: 10,
                    max_count: Some(3),
                    target_mask: TargetMask::GroundAndAir,
                    damage: 14.0,
                    attack_speed: 0.8,
                    range: 4.5,
                    air_bonus_mult: 0.0,
                    attack_kind: AttackKind::SupportAura {
                        aura_radius: 4.5,
                        dmg_bonus: 0.0,
                        bounty_bonus: 0.0,
                        soul_mult: 2.0,
                        passive_gold: 0.0,
                        interest_bonus: 0.0,
                        interest_cap_bonus: 0,
                        core_every_waves: 0,
                    },
                    on_hit_effects: vec![],
                    upgrades_to: vec![],
                    mesh_recipe: TowerMeshRecipe::SoulCrypt,
                    projectile_color: (0.45, 1.0, 0.55),
                },
            ]),
        },
    );

    factions
}

pub fn default_enemies() -> HashMap<EnemyArchetypeId, EnemyConfig> {
    let list = vec![
        EnemyConfig {
            id: EnemyArchetypeId::RatSwarm,
            domain: MovementDomain::Ground,
            base_hp: 32.0,
            base_speed: 2.6,
            base_armor: 0.0,
            slot_weight: 0.5,
            base_bounty: 2,
            is_boss: false,
            is_elite: false,
            regen_pct_per_sec: 0.0,
            split_into: None,
            boss_ability: BossAbilityKind::None,
            scale: 0.58,
            primary_color: (0.52, 0.44, 0.38),
            accent_color: (0.85, 0.58, 0.58),
        },
        EnemyConfig {
            id: EnemyArchetypeId::GoblinRaider,
            domain: MovementDomain::Ground,
            base_hp: 45.0,
            base_speed: 2.1,
            base_armor: 5.0,
            slot_weight: 1.0,
            base_bounty: 4,
            is_boss: false,
            is_elite: false,
            regen_pct_per_sec: 0.0,
            split_into: None,
            boss_ability: BossAbilityKind::None,
            scale: 0.78,
            primary_color: (0.32, 0.62, 0.24),
            accent_color: (0.72, 0.45, 0.20),
        },
        EnemyConfig {
            id: EnemyArchetypeId::BatFlyer,
            domain: MovementDomain::Air,
            base_hp: 120.0,
            base_speed: 2.4,
            base_armor: 0.0,
            slot_weight: 1.0,
            base_bounty: 6,
            is_boss: false,
            is_elite: false,
            regen_pct_per_sec: 0.0,
            split_into: None,
            boss_ability: BossAbilityKind::None,
            scale: 0.75,
            primary_color: (0.28, 0.16, 0.36),
            accent_color: (0.95, 0.28, 0.32),
        },
        EnemyConfig {
            id: EnemyArchetypeId::SlimeSplitter,
            domain: MovementDomain::Ground,
            base_hp: 190.0,
            base_speed: 1.5,
            base_armor: 5.0,
            slot_weight: 1.0,
            base_bounty: 8,
            is_boss: false,
            is_elite: false,
            regen_pct_per_sec: 0.0,
            split_into: Some((EnemyArchetypeId::MiniSlime, 2, 0.35)),
            boss_ability: BossAbilityKind::None,
            scale: 0.92,
            primary_color: (0.22, 0.85, 0.38),
            accent_color: (0.58, 1.0, 0.62),
        },
        EnemyConfig {
            id: EnemyArchetypeId::MiniSlime,
            domain: MovementDomain::Ground,
            base_hp: 65.0,
            base_speed: 1.9,
            base_armor: 0.0,
            slot_weight: 0.5,
            base_bounty: 2,
            is_boss: false,
            is_elite: false,
            regen_pct_per_sec: 0.0,
            split_into: None,
            boss_ability: BossAbilityKind::None,
            scale: 0.52,
            primary_color: (0.34, 0.92, 0.48),
            accent_color: (0.72, 1.0, 0.76),
        },
        EnemyConfig {
            id: EnemyArchetypeId::VampireLord,
            domain: MovementDomain::Ground,
            base_hp: 820.0,
            base_speed: 1.85,
            base_armor: 15.0,
            slot_weight: 1.0,
            base_bounty: 12,
            is_boss: false,
            is_elite: true,
            regen_pct_per_sec: 0.045,
            split_into: None,
            boss_ability: BossAbilityKind::None,
            scale: 0.92,
            primary_color: (0.18, 0.12, 0.22),
            accent_color: (0.92, 0.14, 0.20),
        },
        EnemyConfig {
            id: EnemyArchetypeId::StoneGolem,
            domain: MovementDomain::Ground,
            base_hp: 220.0,
            base_speed: 1.25,
            base_armor: 45.0,
            slot_weight: 2.0,
            base_bounty: 9,
            is_boss: false,
            is_elite: true,
            regen_pct_per_sec: 0.0,
            split_into: None,
            boss_ability: BossAbilityKind::None,
            scale: 1.08,
            primary_color: (0.50, 0.52, 0.55),
            accent_color: (0.95, 0.68, 0.20),
        },
        EnemyConfig {
            id: EnemyArchetypeId::Gargoyle,
            domain: MovementDomain::Air,
            base_hp: 2400.0,
            base_speed: 1.65,
            base_armor: 40.0,
            slot_weight: 2.0,
            base_bounty: 18,
            is_boss: false,
            is_elite: true,
            regen_pct_per_sec: 0.0,
            split_into: None,
            boss_ability: BossAbilityKind::None,
            scale: 1.0,
            primary_color: (0.44, 0.46, 0.52),
            accent_color: (0.82, 0.32, 0.95),
        },
        EnemyConfig {
            id: EnemyArchetypeId::BossRatKing,
            domain: MovementDomain::Ground,
            base_hp: 1350.0,
            base_speed: 1.35,
            base_armor: 30.0,
            slot_weight: 5.0,
            base_bounty: 45,
            is_boss: true,
            is_elite: true,
            regen_pct_per_sec: 0.0,
            split_into: None,
            boss_ability: BossAbilityKind::SummonRats,
            scale: 1.45,
            primary_color: (0.42, 0.30, 0.22),
            accent_color: (0.98, 0.80, 0.18),
        },
        EnemyConfig {
            id: EnemyArchetypeId::BossSlimeTitan,
            domain: MovementDomain::Ground,
            base_hp: 4200.0,
            base_speed: 1.15,
            base_armor: 25.0,
            slot_weight: 5.0,
            base_bounty: 75,
            is_boss: true,
            is_elite: true,
            regen_pct_per_sec: 0.01,
            split_into: Some((EnemyArchetypeId::SlimeSplitter, 3, 0.22)),
            boss_ability: BossAbilityKind::SplitThree,
            scale: 1.65,
            primary_color: (0.14, 0.78, 0.32),
            accent_color: (0.75, 1.0, 0.40),
        },
        EnemyConfig {
            id: EnemyArchetypeId::BossBloodCount,
            domain: MovementDomain::Ground,
            base_hp: 9500.0,
            base_speed: 1.30,
            base_armor: 35.0,
            slot_weight: 5.0,
            base_bounty: 90,
            is_boss: true,
            is_elite: true,
            regen_pct_per_sec: 0.05,
            split_into: None,
            boss_ability: BossAbilityKind::BloodRegenPulse,
            scale: 1.55,
            primary_color: (0.25, 0.06, 0.12),
            accent_color: (1.0, 0.12, 0.18),
        },
        EnemyConfig {
            id: EnemyArchetypeId::BossObsidianWing,
            domain: MovementDomain::Air,
            base_hp: 16000.0,
            base_speed: 1.30,
            base_armor: 50.0,
            slot_weight: 5.0,
            base_bounty: 110,
            is_boss: true,
            is_elite: true,
            regen_pct_per_sec: 0.0,
            split_into: None,
            boss_ability: BossAbilityKind::StoneShield,
            scale: 1.60,
            primary_color: (0.20, 0.20, 0.28),
            accent_color: (0.92, 0.25, 1.0),
        },
        EnemyConfig {
            id: EnemyArchetypeId::BossGolemColossus,
            domain: MovementDomain::Ground,
            base_hp: 26000.0,
            base_speed: 1.10,
            base_armor: 65.0,
            slot_weight: 5.0,
            base_bounty: 120,
            is_boss: true,
            is_elite: true,
            regen_pct_per_sec: 0.0,
            split_into: None,
            boss_ability: BossAbilityKind::StoneShield,
            scale: 1.80,
            primary_color: (0.38, 0.40, 0.45),
            accent_color: (1.0, 0.62, 0.10),
        },
        EnemyConfig {
            id: EnemyArchetypeId::BossAetherDreadlord,
            domain: MovementDomain::Ground,
            base_hp: 68000.0,
            base_speed: 1.25,
            base_armor: 60.0,
            slot_weight: 5.0,
            base_bounty: 150,
            is_boss: true,
            is_elite: true,
            regen_pct_per_sec: 0.025,
            split_into: None,
            boss_ability: BossAbilityKind::SummonVampiresAndBats,
            scale: 1.90,
            primary_color: (0.14, 0.08, 0.26),
            accent_color: (0.30, 1.0, 0.68),
        },
    ];
    list.into_iter().map(|e| (e.id, e)).collect()
}

pub fn default_waves() -> WaveTableConfig {
    let g = |arch, count, hp, bounty, interval, delay| WaveGroupConfig {
        archetype: arch,
        count,
        hp_override: Some(hp),
        bounty_override: Some(bounty),
        spawn_interval: interval,
        start_delay: delay,
    };

    let waves = vec![
        WaveDefConfig {
            wave_number: 1,
            title: "Goblin-Vorhut".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::GoblinRaider, 10, 45.0, 4, 0.75, 0.0)],
        },
        WaveDefConfig {
            wave_number: 2,
            title: "Ratten-Schwarm".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::RatSwarm, 22, 32.0, 2, 0.35, 0.0)],
        },
        WaveDefConfig {
            wave_number: 3,
            title: "Goblin-Plünderer".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::GoblinRaider, 14, 85.0, 5, 0.65, 0.0)],
        },
        WaveDefConfig {
            wave_number: 4,
            title: "Steingolem-Vorhut".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::StoneGolem, 8, 220.0, 9, 1.15, 0.0)],
        },
        WaveDefConfig {
            wave_number: 5,
            title: "BOSS: Rattenkönig".into(),
            is_boss_wave: true,
            has_air: false,
            groups: vec![
                g(EnemyArchetypeId::RatSwarm, 12, 55.0, 3, 0.35, 0.0),
                g(EnemyArchetypeId::BossRatKing, 1, 1350.0, 45, 1.0, 2.0),
            ],
        },
        WaveDefConfig {
            wave_number: 6,
            title: "LUFT: Fledermäuse".into(),
            is_boss_wave: false,
            has_air: true,
            groups: vec![g(EnemyArchetypeId::BatFlyer, 14, 120.0, 6, 0.65, 0.0)],
        },
        WaveDefConfig {
            wave_number: 7,
            title: "Schleim-Splitter".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::SlimeSplitter, 12, 190.0, 8, 0.85, 0.0)],
        },
        WaveDefConfig {
            wave_number: 8,
            title: "Gepanzerte Goblins".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::GoblinRaider, 16, 280.0, 7, 0.55, 0.0)],
        },
        WaveDefConfig {
            wave_number: 9,
            title: "Ratten-Flut".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::RatSwarm, 34, 115.0, 3, 0.25, 0.0)],
        },
        WaveDefConfig {
            wave_number: 10,
            title: "BOSS: Uralter Schleim-Titan".into(),
            is_boss_wave: true,
            has_air: false,
            groups: vec![
                g(EnemyArchetypeId::BossSlimeTitan, 1, 4200.0, 75, 1.0, 0.0),
                g(EnemyArchetypeId::SlimeSplitter, 6, 320.0, 8, 0.9, 1.5),
            ],
        },
        WaveDefConfig {
            wave_number: 11,
            title: "Golem-Marsch".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::StoneGolem, 12, 680.0, 11, 0.95, 0.0)],
        },
        WaveDefConfig {
            wave_number: 12,
            title: "LUFT: Nachtschwingen".into(),
            is_boss_wave: false,
            has_air: true,
            groups: vec![g(EnemyArchetypeId::BatFlyer, 18, 420.0, 8, 0.50, 0.0)],
        },
        WaveDefConfig {
            wave_number: 13,
            title: "Schleim-Woge".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::SlimeSplitter, 16, 620.0, 10, 0.70, 0.0)],
        },
        WaveDefConfig {
            wave_number: 14,
            title: "Goblin- & Ratten-Sturm".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![
                g(EnemyArchetypeId::GoblinRaider, 14, 850.0, 9, 0.55, 0.0),
                g(EnemyArchetypeId::RatSwarm, 22, 310.0, 4, 0.28, 1.2),
            ],
        },
        WaveDefConfig {
            wave_number: 15,
            title: "BOSS: Blutgraf & Vampire".into(),
            is_boss_wave: true,
            has_air: false,
            groups: vec![
                g(EnemyArchetypeId::VampireLord, 14, 820.0, 12, 0.70, 0.0),
                g(EnemyArchetypeId::BossBloodCount, 1, 9500.0, 90, 1.0, 3.0),
            ],
        },
        WaveDefConfig {
            wave_number: 16,
            title: "Golem-Phalanx".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::StoneGolem, 14, 1550.0, 14, 0.85, 0.0)],
        },
        WaveDefConfig {
            wave_number: 17,
            title: "Schleim-Sturm".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::SlimeSplitter, 18, 1300.0, 12, 0.60, 0.0)],
        },
        WaveDefConfig {
            wave_number: 18,
            title: "Vampir-Vorhut".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::VampireLord, 18, 1750.0, 14, 0.65, 0.0)],
        },
        WaveDefConfig {
            wave_number: 19,
            title: "LUFT: Fledermaus-Schwarm".into(),
            is_boss_wave: false,
            has_air: true,
            groups: vec![g(EnemyArchetypeId::BatFlyer, 28, 950.0, 8, 0.35, 0.0)],
        },
        WaveDefConfig {
            wave_number: 20,
            title: "BOSS (LUFT): Obsidianschwinge & Gargoyles".into(),
            is_boss_wave: true,
            has_air: true,
            groups: vec![
                g(EnemyArchetypeId::Gargoyle, 12, 2400.0, 18, 0.85, 0.0),
                g(EnemyArchetypeId::BossObsidianWing, 1, 16000.0, 110, 1.0, 2.5),
            ],
        },
        WaveDefConfig {
            wave_number: 21,
            title: "Gemischte Belagerung I".into(),
            is_boss_wave: false,
            has_air: true,
            groups: vec![
                g(EnemyArchetypeId::StoneGolem, 12, 3100.0, 16, 0.80, 0.0),
                g(EnemyArchetypeId::BatFlyer, 16, 1450.0, 10, 0.45, 1.5),
            ],
        },
        WaveDefConfig {
            wave_number: 22,
            title: "Blut- & Schleimflut".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![
                g(EnemyArchetypeId::VampireLord, 16, 3200.0, 16, 0.65, 0.0),
                g(EnemyArchetypeId::SlimeSplitter, 14, 2600.0, 14, 0.65, 1.0),
            ],
        },
        WaveDefConfig {
            wave_number: 23,
            title: "LUFT: Steinschwingen".into(),
            is_boss_wave: false,
            has_air: true,
            groups: vec![g(EnemyArchetypeId::Gargoyle, 18, 3600.0, 18, 0.70, 0.0)],
        },
        WaveDefConfig {
            wave_number: 24,
            title: "Gemischte Belagerung II".into(),
            is_boss_wave: false,
            has_air: true,
            groups: vec![
                g(EnemyArchetypeId::StoneGolem, 14, 4600.0, 18, 0.75, 0.0),
                g(EnemyArchetypeId::Gargoyle, 12, 3900.0, 18, 0.75, 1.5),
            ],
        },
        WaveDefConfig {
            wave_number: 25,
            title: "DOPPEL-BOSS: Koloss & Blut-Gargoyle".into(),
            is_boss_wave: true,
            has_air: true,
            groups: vec![
                g(EnemyArchetypeId::BossGolemColossus, 1, 26000.0, 80, 1.0, 0.0),
                g(EnemyArchetypeId::BossObsidianWing, 1, 18000.0, 80, 1.0, 2.0),
                g(EnemyArchetypeId::VampireLord, 10, 3800.0, 16, 0.70, 2.5),
            ],
        },
        WaveDefConfig {
            wave_number: 26,
            title: "Elite-Vampir-Ritter".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![g(EnemyArchetypeId::VampireLord, 24, 5600.0, 18, 0.55, 0.0)],
        },
        WaveDefConfig {
            wave_number: 27,
            title: "Mega-Schleime & Schwarm".into(),
            is_boss_wave: false,
            has_air: false,
            groups: vec![
                g(EnemyArchetypeId::SlimeSplitter, 20, 6200.0, 18, 0.60, 0.0),
                g(EnemyArchetypeId::RatSwarm, 20, 2400.0, 8, 0.25, 1.5),
            ],
        },
        WaveDefConfig {
            wave_number: 28,
            title: "LUFT: Frost-Gargoyles".into(),
            is_boss_wave: false,
            has_air: true,
            groups: vec![g(EnemyArchetypeId::Gargoyle, 22, 7400.0, 20, 0.60, 0.0)],
        },
        WaveDefConfig {
            wave_number: 29,
            title: "Vorhut des Dreadlords".into(),
            is_boss_wave: false,
            has_air: true,
            groups: vec![
                g(EnemyArchetypeId::StoneGolem, 14, 9200.0, 20, 0.70, 0.0),
                g(EnemyArchetypeId::VampireLord, 12, 7800.0, 20, 0.60, 1.0),
                g(EnemyArchetypeId::Gargoyle, 10, 8200.0, 20, 0.70, 2.0),
            ],
        },
        WaveDefConfig {
            wave_number: 30,
            title: "ENDBOSS: Aether-Dreadlord".into(),
            is_boss_wave: true,
            has_air: true,
            groups: vec![
                g(EnemyArchetypeId::BossAetherDreadlord, 1, 68000.0, 150, 1.0, 0.0),
                g(EnemyArchetypeId::Gargoyle, 6, 6500.0, 18, 1.2, 2.0),
                g(EnemyArchetypeId::VampireLord, 6, 6000.0, 18, 1.2, 3.0),
            ],
        },
    ];

    WaveTableConfig {
        waves,
        endless_hp_growth: 1.18,
        endless_armor_per_wave: 3.0,
        endless_speed_per_wave: 0.015,
        endless_speed_cap: 1.45,
        endless_bounty_growth: 1.05,
    }
}
