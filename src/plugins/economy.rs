use bevy::prelude::*;

use crate::types::*;

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_start_match_economy_init,
                passive_income_and_banner_tick,
                handle_kill_and_freeze_economy,
                handle_wave_completed_economy,
                monitor_slot_limit_and_grace,
            )
                .chain()
                .in_set(GameSet::LeakEconomySet)
                .run_if(in_state(AppState::Playing)),
        );
    }
}

fn handle_start_match_economy_init(
    mut start_requests: MessageReader<StartMatchRequest>,
    configs: Res<GameConfigs>,
    mut economy: ResMut<EconomyState>,
) {
    for req in start_requests.read() {
        let diff_cfg = configs.difficulty_cfg(req.difficulty);
        let starting_special = configs
            .get_faction(req.faction_id)
            .map(|f| f.starting_currency)
            .unwrap_or(1);

        economy.gold = diff_cfg.start_gold;
        economy.gold_frac = 0.0;
        economy.special_currency = starting_special;
        economy.total_kills = 0;
        economy.normal_kill_accum = 0;
        economy.last_interest = 0;
        economy.last_wave_bonus = 0;
        economy.last_early_bonus = 0;
        economy.interest_rate_bonus = 0.0;
        economy.interest_cap_bonus = 0;
        economy.passive_gold_bonus = 0.0;
        economy.score = 0;
        economy.match_elapsed_secs = 0.0;
        economy.status_banner = None;
    }
}

fn passive_income_and_banner_tick(
    time: Res<Time>,
    speed: Res<GameSpeed>,
    configs: Res<GameConfigs>,
    wave: Res<WaveState>,
    mut economy: ResMut<EconomyState>,
) {
    let real_dt = time.delta_secs();
    if let Some((_, ref mut ttl)) = economy.status_banner {
        *ttl -= real_dt;
        if *ttl <= 0.0 {
            economy.status_banner = None;
        }
    }

    let dt = real_dt * speed.multiplier();
    if dt <= 0.0 {
        return;
    }

    economy.match_elapsed_secs += dt;

    if wave.phase == WavePhase::WaveActive {
        let rate = configs.balance.passive_gold_per_sec + economy.passive_gold_bonus;
        economy.gold_frac += rate * dt;
        if economy.gold_frac >= 1.0 {
            let whole = economy.gold_frac.floor() as u32;
            economy.gold += whole;
            economy.gold_frac -= whole as f32;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_kill_and_freeze_economy(
    configs: Res<GameConfigs>,
    match_cfg: Res<MatchConfig>,
    mut economy: ResMut<EconomyState>,
    mut killed_reader: MessageReader<EnemyKilledMsg>,
    mut freeze_reader: MessageReader<FreezeTriggeredMsg>,
    mut towers: Query<(&mut Tower, &Transform), Without<PendingDespawn>>,
    mut popup_writer: MessageWriter<SpawnPopupMsg>,
) {
    let diff_cfg = configs.difficulty_cfg(match_cfg.difficulty);
    let faction_cfg = configs.get_faction(match_cfg.faction_id);

    // Pre-collect aura positions for Gold Hoard Shrine and Soul Crypt
    let mut shrine_auras: Vec<(Vec3, f32, f32)> = Vec::new();
    let mut crypt_entities: Vec<(Vec3, f32, f32)> = Vec::new();

    for (tower, tf) in &towers {
        if let Some(cfg) = configs.get_tower(tower.faction, &tower.id)
            && let AttackKind::SupportAura {
                aura_radius,
                bounty_bonus,
                soul_mult,
                ..
            } = cfg.attack_kind
        {
            if bounty_bonus > 0.0 {
                shrine_auras.push((tf.translation, aura_radius, bounty_bonus));
            }
            if soul_mult > 1.0 {
                crypt_entities.push((tf.translation, aura_radius, soul_mult));
            }
        }
    }

    for ev in killed_reader.read() {
        // Bounty boosted +20% if killed within a GoldHoardShrine aura
        let mut best_bounty_bonus = 0.0_f32;
        for &(s_pos, radius, bonus) in &shrine_auras {
            let dx = ev.pos.x - s_pos.x;
            let dz = ev.pos.z - s_pos.z;
            if dx * dx + dz * dz <= radius * radius {
                best_bounty_bonus = best_bounty_bonus.max(bonus);
            }
        }

        let effective_bounty = ((ev.bounty as f32) * (1.0 + best_bounty_bonus)).round() as u32;
        economy.gold += effective_bounty;
        economy.total_kills += 1;

        let base_score = if ev.is_boss {
            500.0
        } else if ev.is_elite {
            120.0
        } else {
            35.0
        };
        economy.score += (base_score * diff_cfg.score_mult).round() as u64;

        // Check if within a SoulCrypt aura (doubles Souls, +5 bonus gold every 8 kills in aura)
        let mut soul_multiplier = 1.0_f32;
        for &(c_pos, radius, s_mult) in &crypt_entities {
            let dx = ev.pos.x - c_pos.x;
            let dz = ev.pos.z - c_pos.z;
            if dx * dx + dz * dz <= radius * radius {
                soul_multiplier = soul_multiplier.max(s_mult);
            }
        }

        // Increment aura_kill_counter on SoulCrypt towers in range of kill
        for (mut tower, tf) in &mut towers {
            if tower.id == "soul_crypt"
                && let Some(cfg) = configs.get_tower(tower.faction, &tower.id)
                && let AttackKind::SupportAura { aura_radius, .. } = cfg.attack_kind
            {
                let dx = ev.pos.x - tf.translation.x;
                let dz = ev.pos.z - tf.translation.z;
                if dx * dx + dz * dz <= aura_radius * aura_radius {
                    tower.aura_kill_counter += 1;
                    if tower.aura_kill_counter.is_multiple_of(8) {
                        economy.gold += 5;
                        popup_writer.write(SpawnPopupMsg {
                            pos: tf.translation + Vec3::Y * 1.5,
                            text: "+5g Krypta".to_string(),
                            color: Color::srgb(0.45, 1.0, 0.55),
                        });
                    }
                }
            }
        }

        // Apply faction currency kill rules
        if let Some(fcfg) = faction_cfg {
            match fcfg.currency_rule {
                CurrencyRule::EveryNWavesAndBoss { boss_amount, .. } => {
                    if ev.is_boss {
                        economy.special_currency += boss_amount;
                        popup_writer.write(SpawnPopupMsg {
                            pos: ev.pos + Vec3::Y * 1.6,
                            text: format!("+{boss_amount} Essenz"),
                            color: FactionId::ElementalCircle.accent_color(),
                        });
                    }
                }
                CurrencyRule::EveryNWavesAndTreasury { .. } => {}
                CurrencyRule::BurnKillsAndBoss {
                    burn_kill_amount,
                    normal_kills_per_unit,
                    boss_amount,
                } => {
                    let mut gained = 0_u32;
                    if ev.is_boss {
                        gained += boss_amount;
                    } else if ev.was_burning {
                        gained += burn_kill_amount;
                    } else {
                        economy.normal_kill_accum += 1;
                        if normal_kills_per_unit > 0
                            && economy.normal_kill_accum >= normal_kills_per_unit
                        {
                            economy.normal_kill_accum -= normal_kills_per_unit;
                            gained += 1;
                        }
                    }
                    if gained > 0 {
                        economy.special_currency += gained;
                        if ev.is_boss || gained >= 2 {
                            popup_writer.write(SpawnPopupMsg {
                                pos: ev.pos + Vec3::Y * 1.5,
                                text: format!("+{gained} Glut"),
                                color: FactionId::DragonClan.accent_color(),
                            });
                        }
                    }
                }
                CurrencyRule::SlowedKillsAndFreeze {
                    slowed_kill_amount,
                    ..
                } => {
                    if ev.was_slowed_or_frozen && slowed_kill_amount > 0 {
                        economy.special_currency += slowed_kill_amount;
                    }
                }
                CurrencyRule::EveryKill {
                    normal_amount,
                    elite_amount,
                    boss_amount,
                } => {
                    let base_souls = if ev.is_boss {
                        boss_amount
                    } else if ev.is_elite {
                        elite_amount
                    } else {
                        normal_amount
                    };
                    let total_souls = ((base_souls as f32) * soul_multiplier).round() as u32;
                    economy.special_currency += total_souls;
                    if ev.is_boss {
                        popup_writer.write(SpawnPopupMsg {
                            pos: ev.pos + Vec3::Y * 1.6,
                            text: format!("+{total_souls} Seelen"),
                            color: FactionId::Necropolis.accent_color(),
                        });
                    }
                }
            }
        }
    }

    // Frostguard: +1 Frost Shard per FreezeTriggeredMsg
    for ev in freeze_reader.read() {
        if let Some(fcfg) = faction_cfg
            && let CurrencyRule::SlowedKillsAndFreeze {
                freeze_proc_amount, ..
            } = fcfg.currency_rule
            && freeze_proc_amount > 0
        {
            economy.special_currency += freeze_proc_amount;
            popup_writer.write(SpawnPopupMsg {
                pos: ev.pos + Vec3::Y * 1.4,
                text: format!("+{freeze_proc_amount} Splitter"),
                color: FactionId::Frostguard.accent_color(),
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_wave_completed_economy(
    configs: Res<GameConfigs>,
    match_cfg: Res<MatchConfig>,
    mut economy: ResMut<EconomyState>,
    mut completed_reader: MessageReader<WaveCompletedMsg>,
    towers: Query<&Tower, Without<PendingDespawn>>,
    mut sfx_writer: MessageWriter<PlaySfxMsg>,
) {
    let diff_cfg = configs.difficulty_cfg(match_cfg.difficulty);
    let faction_cfg = configs.get_faction(match_cfg.faction_id);

    for msg in completed_reader.read() {
        let wave = msg.wave;
        let wave_bonus = configs.balance.wave_bonus_base + configs.balance.wave_bonus_per_wave * wave;
        let interest_rate = configs.balance.interest_rate + economy.interest_rate_bonus;
        let interest_cap = configs.balance.interest_cap + economy.interest_cap_bonus;
        let interest = ((economy.gold as f32) * interest_rate)
            .floor()
            .min(interest_cap as f32)
            .max(0.0) as u32;

        economy.gold += wave_bonus + interest;
        economy.last_wave_bonus = wave_bonus;
        economy.last_interest = interest;
        economy.score += ((wave as f32 * 250.0) * diff_cfg.score_mult).round() as u64;

        // Wave-milestone faction currency
        if let Some(fcfg) = faction_cfg {
            match fcfg.currency_rule {
                CurrencyRule::EveryNWavesAndBoss {
                    every_waves,
                    wave_amount,
                    ..
                } => {
                    if every_waves > 0 && wave.is_multiple_of(every_waves) {
                        economy.special_currency += wave_amount;
                    }
                }
                CurrencyRule::EveryNWavesAndTreasury {
                    every_waves,
                    wave_amount,
                } => {
                    if every_waves > 0 && wave.is_multiple_of(every_waves) {
                        economy.special_currency += wave_amount;
                    }
                    // Each Guild Treasury generates +1 Core every 4 waves
                    let mut treasury_cores = 0_u32;
                    for tower in &towers {
                        if let Some(cfg) = configs.get_tower(tower.faction, &tower.id)
                            && let AttackKind::SupportAura {
                                core_every_waves, ..
                            } = cfg.attack_kind
                            && core_every_waves > 0
                            && wave.is_multiple_of(core_every_waves)
                        {
                            treasury_cores += 1;
                        }
                    }
                    economy.special_currency += treasury_cores;
                }
                _ => {}
            }
        }

        sfx_writer.write(PlaySfxMsg {
            kind: SfxEventKind::GoldCoin,
            volume_scale: 1.0,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn monitor_slot_limit_and_grace(
    time: Res<Time>,
    speed: Res<GameSpeed>,
    configs: Res<GameConfigs>,
    mut wave: ResMut<WaveState>,
    enemies: Query<&Enemy, Without<PendingDespawn>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut sfx_writer: MessageWriter<PlaySfxMsg>,
    mut save_writer: MessageWriter<SavePersistenceMsg>,
) {
    let total_weight: f32 = enemies
        .iter()
        .filter(|e| e.hp > 0.0)
        .map(|e| e.slot_weight)
        .sum();
    wave.active_slots = total_weight;

    let occupied_slots = total_weight.ceil() as u32;
    if occupied_slots > wave.max_slots {
        if !wave.grace_active {
            wave.grace_active = true;
            wave.grace_timer = configs.balance.limit_grace_period_secs;
            sfx_writer.write(PlaySfxMsg {
                kind: SfxEventKind::LimitWarning,
                volume_scale: 1.0,
            });
        }
        let dt = time.delta_secs() * speed.multiplier();
        wave.grace_timer = (wave.grace_timer - dt).max(0.0);
        if wave.grace_timer <= 0.0 {
            next_state.set(AppState::GameOver);
            sfx_writer.write(PlaySfxMsg {
                kind: SfxEventKind::DefeatHorn,
                volume_scale: 1.0,
            });
            save_writer.write(SavePersistenceMsg);
        }
    } else {
        wave.grace_active = false;
        wave.grace_timer = configs.balance.limit_grace_period_secs;
    }
}
