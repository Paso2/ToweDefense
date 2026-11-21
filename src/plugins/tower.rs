use bevy::prelude::*;

use crate::plugins::pathfinding::validate_placement;
use crate::types::*;

pub struct TowerPlugin;

impl Plugin for TowerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_build_tower_requests,
                handle_upgrade_tower_requests,
                handle_sell_tower_requests,
                recompute_support_economy_bonuses,
            )
                .chain()
                .in_set(GameSet::BuildSet)
                .run_if(in_state(AppState::Playing)),
        )
        .add_systems(
            Update,
            acquire_tower_targets
                .in_set(GameSet::TargetingSet)
                .run_if(in_state(AppState::Playing)),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_build_tower_requests(
    mut commands: Commands,
    mut build_requests: MessageReader<BuildTowerRequest>,
    configs: Res<GameConfigs>,
    mut grid: ResMut<GridMap>,
    mut economy: ResMut<EconomyState>,
    wave: Res<WaveState>,
    ground_enemies: Query<(&Transform, &Enemy), Without<PendingDespawn>>,
    existing_towers: Query<&Tower, Without<PendingDespawn>>,
    mut built_writer: MessageWriter<TowerBuiltMsg>,
    mut vfx_writer: MessageWriter<SpawnVfxMsg>,
    mut sfx_writer: MessageWriter<PlaySfxMsg>,
) {
    let requests: Vec<BuildTowerRequest> = build_requests.read().cloned().collect();
    if requests.is_empty() {
        return;
    }

    let ground_positions: Vec<Vec3> = ground_enemies
        .iter()
        .filter(|(_, e)| e.domain == MovementDomain::Ground && e.hp > 0.0)
        .map(|(tf, _)| tf.translation)
        .collect();

    // Track counts across multiple requests in the same frame
    let mut built_counts_delta: Vec<(String, u32)> = Vec::new();

    for req in requests {
        let Some(cfg) = configs.get_tower(req.faction, &req.tower_id) else {
            warn!(
                "BuildTowerRequest ignored: unknown tower '{}' for {:?}",
                req.tower_id, req.faction
            );
            continue;
        };

        // Check max_count constraint (unless free)
        if !req.free
            && let Some(max_count) = cfg.max_count
        {
            let existing = existing_towers
                .iter()
                .filter(|t| t.id == req.tower_id)
                .count() as u32;
            let added_this_frame = built_counts_delta
                .iter()
                .filter(|(id, _)| id == &req.tower_id)
                .map(|(_, c)| *c)
                .sum::<u32>();
            if existing + added_this_frame >= max_count {
                economy.status_banner =
                    Some((BuildBlockReason::MaxCountReached.message().to_string(), 2.0));
                sfx_writer.write(PlaySfxMsg {
                    kind: SfxEventKind::UiClick,
                    volume_scale: 0.9,
                });
                continue;
            }
        }

        // Check cost constraint (unless free)
        if !req.free
            && (economy.gold < cfg.gold_cost || economy.special_currency < cfg.special_cost)
        {
            economy.status_banner = Some((
                BuildBlockReason::InsufficientFunds.message().to_string(),
                2.0,
            ));
            sfx_writer.write(PlaySfxMsg {
                kind: SfxEventKind::UiClick,
                volume_scale: 0.9,
            });
            continue;
        }

        let is_wave_active = wave.phase == WavePhase::WaveActive;
        if let Err(reason) = validate_placement(&grid, req.coord, &ground_positions, is_wave_active)
        {
            economy.status_banner = Some((reason.message().to_string(), 2.0));
            sfx_writer.write(PlaySfxMsg {
                kind: SfxEventKind::UiClick,
                volume_scale: 0.9,
            });
            continue;
        }

        // Deduct resources
        if !req.free {
            economy.gold -= cfg.gold_cost;
            economy.special_currency -= cfg.special_cost;
        }

        let world_pos = grid.tile_to_world(req.coord);
        let built_in_prep_wave = if wave.phase == WavePhase::BuildPrep {
            Some(wave.current_wave)
        } else {
            None
        };
        let default_priority = if cfg.target_mask == TargetMask::AirOnly {
            TargetPriority::AirPriority
        } else {
            TargetPriority::First
        };

        let tower_entity = commands
            .spawn((
                GameplayEntity,
                Tower {
                    id: cfg.id.clone(),
                    faction: req.faction,
                    coord: req.coord,
                    tier: cfg.tier,
                    invested_gold: if req.free { 0 } else { cfg.gold_cost },
                    invested_special: if req.free { 0 } else { cfg.special_cost },
                    built_in_prep_wave,
                    priority: default_priority,
                    cooldown: 0.0,
                    ramp_stacks: 0,
                    shot_counter: 0,
                    kills: 0,
                    aura_kill_counter: 0,
                    total_damage: 0.0,
                    dps_estimate: estimate_tower_dps(cfg),
                    target: None,
                    recoil: 0.0,
                    visual_spawned: false,
                },
                Transform::from_translation(world_pos),
                Visibility::default(),
            ))
            .id();

        grid.set(req.coord, TileKind::Tower(tower_entity));
        built_counts_delta.push((req.tower_id.clone(), 1));

        built_writer.write(TowerBuiltMsg {
            entity: tower_entity,
            coord: req.coord,
            tower_id: cfg.id.clone(),
            faction: req.faction,
        });

        let accent = configs
            .get_faction(req.faction)
            .map(|f| Color::srgb(f.accent_color.0, f.accent_color.1, f.accent_color.2))
            .unwrap_or_else(|| req.faction.accent_color());

        vfx_writer.write(SpawnVfxMsg {
            kind: VfxKind::BuildDust,
            pos: world_pos + Vec3::Y * 0.2,
            target_pos: None,
            color: accent,
            radius: 0.9,
        });

        sfx_writer.write(PlaySfxMsg {
            kind: SfxEventKind::BuildTower,
            volume_scale: 1.0,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_upgrade_tower_requests(
    mut upgrade_requests: MessageReader<UpgradeTowerRequest>,
    configs: Res<GameConfigs>,
    mut economy: ResMut<EconomyState>,
    mut towers: Query<(&mut Tower, &Transform), Without<PendingDespawn>>,
    mut upgraded_writer: MessageWriter<TowerUpgradedMsg>,
    mut vfx_writer: MessageWriter<SpawnVfxMsg>,
    mut sfx_writer: MessageWriter<PlaySfxMsg>,
) {
    for req in upgrade_requests.read() {
        let Ok((mut tower, tf)) = towers.get_mut(req.tower_entity) else {
            continue;
        };

        let Some(current_cfg) = configs.get_tower(tower.faction, &tower.id) else {
            continue;
        };

        if !req.free && !current_cfg.upgrades_to.iter().any(|id| id == &req.target_tower_id) {
            continue;
        }

        let Some(next_cfg) = configs.get_tower(tower.faction, &req.target_tower_id) else {
            continue;
        };

        if !req.free
            && (economy.gold < next_cfg.gold_cost
                || economy.special_currency < next_cfg.special_cost)
        {
            economy.status_banner = Some((
                BuildBlockReason::InsufficientFunds.message().to_string(),
                2.0,
            ));
            sfx_writer.write(PlaySfxMsg {
                kind: SfxEventKind::UiClick,
                volume_scale: 0.9,
            });
            continue;
        }

        if !req.free {
            economy.gold -= next_cfg.gold_cost;
            economy.special_currency -= next_cfg.special_cost;
            tower.invested_gold += next_cfg.gold_cost;
            tower.invested_special += next_cfg.special_cost;
        }

        let old_id = tower.id.clone();
        tower.id = next_cfg.id.clone();
        tower.tier = next_cfg.tier;
        tower.cooldown = 0.0;
        tower.ramp_stacks = 0;
        tower.shot_counter = 0;
        tower.target = None;
        tower.dps_estimate = estimate_tower_dps(next_cfg);
        tower.visual_spawned = false;

        let world_pos = tf.translation;
        let faction = tower.faction;
        let coord = tower.coord;
        let new_id = tower.id.clone();

        upgraded_writer.write(TowerUpgradedMsg {
            entity: req.tower_entity,
            coord,
            old_id,
            new_id,
            faction,
        });

        let accent = configs
            .get_faction(faction)
            .map(|f| Color::srgb(f.accent_color.0, f.accent_color.1, f.accent_color.2))
            .unwrap_or_else(|| faction.accent_color());

        vfx_writer.write(SpawnVfxMsg {
            kind: VfxKind::UpgradeBurst,
            pos: world_pos + Vec3::Y * 0.6,
            target_pos: None,
            color: accent,
            radius: 1.1,
        });

        sfx_writer.write(PlaySfxMsg {
            kind: SfxEventKind::UpgradeTower,
            volume_scale: 1.0,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_sell_tower_requests(
    mut commands: Commands,
    mut sell_requests: MessageReader<SellTowerRequest>,
    configs: Res<GameConfigs>,
    mut grid: ResMut<GridMap>,
    mut economy: ResMut<EconomyState>,
    wave: Res<WaveState>,
    towers: Query<(&Tower, &Transform), Without<PendingDespawn>>,
    mut sold_writer: MessageWriter<TowerSoldMsg>,
    mut popup_writer: MessageWriter<SpawnPopupMsg>,
    mut sfx_writer: MessageWriter<PlaySfxMsg>,
) {
    let balance = &configs.balance;
    for req in sell_requests.read() {
        let Ok((tower, tf)) = towers.get(req.tower_entity) else {
            continue;
        };

        let refund_rate = if wave.phase == WavePhase::BuildPrep {
            if tower.built_in_prep_wave == Some(wave.current_wave) {
                balance.sell_refund_same_prep
            } else {
                balance.sell_refund_prep
            }
        } else {
            balance.sell_refund_active_wave
        };

        let refund_gold = (tower.invested_gold as f32 * refund_rate).round() as u32;
        economy.gold += refund_gold;

        let coord = tower.coord;
        grid.set(coord, TileKind::Buildable);
        if wave.phase == WavePhase::WaveActive {
            grid.set_reclaim_timer(coord, balance.reclaim_cooldown_secs);
        }

        if let Ok(mut ec) = commands.get_entity(req.tower_entity) {
            ec.insert(PendingDespawn);
        }

        sold_writer.write(TowerSoldMsg { coord, refund_gold });

        if refund_gold > 0 {
            popup_writer.write(SpawnPopupMsg {
                pos: tf.translation + Vec3::Y * 1.4,
                text: format!("+{refund_gold}g"),
                color: Color::srgb(1.0, 0.88, 0.25),
            });
        }

        sfx_writer.write(PlaySfxMsg {
            kind: SfxEventKind::SellTower,
            volume_scale: 1.0,
        });
    }
}

fn recompute_support_economy_bonuses(
    configs: Res<GameConfigs>,
    mut economy: ResMut<EconomyState>,
    towers: Query<&Tower, Without<PendingDespawn>>,
) {
    let mut passive_gold_bonus = 0.0_f32;
    let mut interest_rate_bonus = 0.0_f32;
    let mut interest_cap_bonus = 0_u32;

    for tower in &towers {
        if let Some(cfg) = configs.get_tower(tower.faction, &tower.id)
            && let AttackKind::SupportAura {
                passive_gold,
                interest_bonus,
                interest_cap_bonus: cap_bonus,
                ..
            } = cfg.attack_kind
        {
            passive_gold_bonus += passive_gold;
            interest_rate_bonus += interest_bonus;
            interest_cap_bonus += cap_bonus;
        }
    }

    economy.passive_gold_bonus = passive_gold_bonus;
    economy.interest_rate_bonus = interest_rate_bonus;
    economy.interest_cap_bonus = interest_cap_bonus;
}

fn acquire_tower_targets(
    configs: Res<GameConfigs>,
    grid: Res<GridMap>,
    mut towers: Query<(&mut Tower, &Transform), Without<PendingDespawn>>,
    enemies: Query<(Entity, &Enemy, &Transform), Without<PendingDespawn>>,
) {
    // Cache stage target center positions to estimate remaining distance for Air enemies accurately
    let stage_centers: Vec<Vec3> = (0..grid.stage_count())
        .map(|s| grid.zone_center_world(grid.stage_target_tiles(s)))
        .collect();

    for (mut tower, tower_tf) in &mut towers {
        let Some(cfg) = configs.get_tower(tower.faction, &tower.id) else {
            tower.target = None;
            continue;
        };

        if cfg.attack_speed <= 0.0 || cfg.damage <= 0.0 {
            tower.target = None;
            continue;
        }

        let tower_pos = tower_tf.translation;
        let range_sq = cfg.range * cfg.range;

        let mut best_entity: Option<Entity> = None;
        let mut best_rank: Option<TargetScore> = None;

        for (enemy_entity, enemy, enemy_tf) in &enemies {
            if enemy.hp <= 0.0 || !cfg.target_mask.can_target(enemy.domain) {
                continue;
            }

            let dx = enemy_tf.translation.x - tower_pos.x;
            let dz = enemy_tf.translation.z - tower_pos.z;
            let dist_sq = dx * dx + dz * dz;
            if dist_sq > range_sq {
                continue;
            }

            let progress_dist = if enemy.domain == MovementDomain::Air {
                stage_centers
                    .get(enemy.current_stage)
                    .map(|c| {
                        let ex = enemy_tf.translation.x - c.x;
                        let ez = enemy_tf.translation.z - c.z;
                        (ex * ex + ez * ez).sqrt()
                    })
                    .unwrap_or(enemy.last_stage_dist as f32)
            } else {
                enemy.last_stage_dist as f32
            };

            let score = TargetScore::from_enemy(
                tower.priority,
                enemy,
                progress_dist,
                dist_sq,
            );

            let is_better = match best_rank {
                None => true,
                Some(ref current_best) => score.is_better_than(current_best),
            };

            if is_better {
                best_rank = Some(score);
                best_entity = Some(enemy_entity);
            }
        }

        tower.target = best_entity;
    }
}

#[derive(Debug, Clone, Copy)]
enum TargetScore {
    First {
        stage: usize,
        stage_dist: f32,
    },
    Strongest {
        hp: f32,
        stage: usize,
        stage_dist: f32,
    },
    Weakest {
        hp: f32,
        stage: usize,
        stage_dist: f32,
    },
    Closest {
        dist_sq: f32,
    },
    AirPriority {
        is_air: bool,
        stage: usize,
        stage_dist: f32,
    },
}

impl TargetScore {
    fn from_enemy(
        priority: TargetPriority,
        enemy: &Enemy,
        stage_dist: f32,
        dist_sq: f32,
    ) -> Self {
        match priority {
            TargetPriority::First => Self::First {
                stage: enemy.current_stage,
                stage_dist,
            },
            TargetPriority::Strongest => Self::Strongest {
                hp: enemy.hp,
                stage: enemy.current_stage,
                stage_dist,
            },
            TargetPriority::Weakest => Self::Weakest {
                hp: enemy.hp,
                stage: enemy.current_stage,
                stage_dist,
            },
            TargetPriority::Closest => Self::Closest { dist_sq },
            TargetPriority::AirPriority => Self::AirPriority {
                is_air: enemy.domain == MovementDomain::Air,
                stage: enemy.current_stage,
                stage_dist,
            },
        }
    }

    fn is_better_than(&self, other: &Self) -> bool {
        match (*self, *other) {
            (
                Self::First {
                    stage: s1,
                    stage_dist: d1,
                },
                Self::First {
                    stage: s2,
                    stage_dist: d2,
                },
            ) => s1 > s2 || (s1 == s2 && d1 < d2),
            (
                Self::Strongest {
                    hp: h1,
                    stage: s1,
                    stage_dist: d1,
                },
                Self::Strongest {
                    hp: h2,
                    stage: s2,
                    stage_dist: d2,
                },
            ) => h1 > h2 || ((h1 - h2).abs() < f32::EPSILON && (s1 > s2 || (s1 == s2 && d1 < d2))),
            (
                Self::Weakest {
                    hp: h1,
                    stage: s1,
                    stage_dist: d1,
                },
                Self::Weakest {
                    hp: h2,
                    stage: s2,
                    stage_dist: d2,
                },
            ) => h1 < h2 || ((h1 - h2).abs() < f32::EPSILON && (s1 > s2 || (s1 == s2 && d1 < d2))),
            (Self::Closest { dist_sq: a }, Self::Closest { dist_sq: b }) => a < b,
            (
                Self::AirPriority {
                    is_air: a1,
                    stage: s1,
                    stage_dist: d1,
                },
                Self::AirPriority {
                    is_air: a2,
                    stage: s2,
                    stage_dist: d2,
                },
            ) => {
                if a1 != a2 {
                    a1
                } else {
                    s1 > s2 || (s1 == s2 && d1 < d2)
                }
            }
            _ => false,
        }
    }
}

pub fn estimate_tower_dps(cfg: &TowerConfig) -> f32 {
    if cfg.attack_speed <= 0.0 || cfg.damage <= 0.0 {
        return 0.0;
    }
    let mut base = cfg.damage * cfg.attack_speed;
    match cfg.attack_kind {
        AttackKind::MultiShot { count, .. } => {
            base *= count as f32;
        }
        AttackKind::ChainLightning {
            bounces,
            decay_per_bounce,
            ..
        } => {
            let mut sum_mult = 0.0;
            for i in 0..bounces {
                sum_mult += (1.0 - decay_per_bounce).powi(i as i32);
            }
            base *= sum_mult.max(1.0);
        }
        _ => {}
    }
    for eff in &cfg.on_hit_effects {
        match *eff {
            OnHitEffect::Burn { dps, .. } | OnHitEffect::Poison { dps, .. } => {
                base += dps;
            }
            OnHitEffect::Crit { chance, mult } => {
                base *= 1.0 + chance * (mult - 1.0);
            }
            _ => {}
        }
    }
    base
}
