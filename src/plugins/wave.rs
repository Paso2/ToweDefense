use bevy::prelude::*;

use crate::types::*;

pub struct WavePlugin;

impl Plugin for WavePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_start_match_wave_init,
                wave_progression_and_spawning,
                boss_abilities_system,
            )
                .chain()
                .in_set(GameSet::SpawnSet)
                .run_if(in_state(AppState::Playing)),
        )
        .add_systems(
            Update,
            (enemy_movement_system, check_wave_completion_system)
                .chain()
                .in_set(GameSet::MovementSet)
                .run_if(in_state(AppState::Playing)),
        );
    }
}

fn handle_start_match_wave_init(
    mut start_requests: MessageReader<StartMatchRequest>,
    configs: Res<GameConfigs>,
    mut wave: ResMut<WaveState>,
) {
    for req in start_requests.read() {
        let diff_cfg = configs.difficulty_cfg(req.difficulty);
        let preview = get_or_generate_wave(&configs, 1);

        wave.current_wave = 0;
        wave.max_standard_waves = configs.waves.waves.len() as u32;
        if wave.max_standard_waves == 0 {
            wave.max_standard_waves = 30;
        }
        wave.phase = WavePhase::BuildPrep;
        wave.prep_timer = configs.balance.prep_time_first_wave_secs;
        wave.active_slots = 0.0;
        wave.max_slots = diff_cfg.max_slots;
        wave.grace_timer = configs.balance.limit_grace_period_secs;
        wave.grace_active = false;
        wave.spawn_queue.clear();
        wave.wave_spawn_done = true;
        wave.active_boss = None;
        wave.next_wave_has_air = preview.has_air;
        wave.next_wave_title = format!("Welle {}: {}", preview.wave_number, preview.title);
        wave.next_wave_is_boss = preview.is_boss_wave;
        wave.leaked_total = 0;
    }
}

#[allow(clippy::too_many_arguments)]
fn wave_progression_and_spawning(
    mut commands: Commands,
    time: Res<Time>,
    speed: Res<GameSpeed>,
    configs: Res<GameConfigs>,
    match_cfg: Res<MatchConfig>,
    grid: Res<GridMap>,
    mut wave: ResMut<WaveState>,
    mut economy: ResMut<EconomyState>,
    mut early_requests: MessageReader<SendWaveEarlyRequest>,
    mut wave_started_writer: MessageWriter<WaveStartedMsg>,
    mut sfx_writer: MessageWriter<PlaySfxMsg>,
    mut popup_writer: MessageWriter<SpawnPopupMsg>,
) {
    let dt = time.delta_secs() * speed.multiplier();
    let early_requested = !early_requests.is_empty();
    early_requests.clear();

    match wave.phase {
        WavePhase::BuildPrep => {
            if dt > 0.0 {
                wave.prep_timer = (wave.prep_timer - dt).max(0.0);
            }

            if early_requested || wave.prep_timer <= 0.0 {
                let next_wave = wave.current_wave + 1;
                if early_requested && wave.prep_timer > 0.5 {
                    let rate = configs.balance.early_send_base_per_sec
                        + configs.balance.early_send_wave_scale * next_wave as f32;
                    let early_bonus = (wave.prep_timer * rate).floor().max(1.0) as u32;
                    economy.gold += early_bonus;
                    economy.last_early_bonus = early_bonus;

                    let center = grid
                        .spawns
                        .first()
                        .map(|z| grid.zone_center_world(z))
                        .unwrap_or(Vec3::ZERO);
                    popup_writer.write(SpawnPopupMsg {
                        pos: center + Vec3::Y * 1.6,
                        text: format!("Early +{early_bonus}g"),
                        color: Color::srgb(1.0, 0.88, 0.25),
                    });
                    sfx_writer.write(PlaySfxMsg {
                        kind: SfxEventKind::GoldCoin,
                        volume_scale: 0.9,
                    });
                } else {
                    economy.last_early_bonus = 0;
                }

                start_wave(
                    next_wave,
                    &configs,
                    &grid,
                    &mut wave,
                    &mut wave_started_writer,
                    &mut sfx_writer,
                );
            }
        }
        WavePhase::WaveActive => {
            if dt <= 0.0 || wave.spawn_queue.is_empty() {
                if wave.spawn_queue.is_empty() {
                    wave.wave_spawn_done = true;
                }
                return;
            }

            let diff_cfg = configs.difficulty_cfg(match_cfg.difficulty);
            let mut spawned_indices: Vec<usize> = Vec::new();

            for (idx, pending) in wave.spawn_queue.iter_mut().enumerate() {
                pending.delay_remaining -= dt;
                if pending.delay_remaining <= 0.0 {
                    spawned_indices.push(idx);
                }
            }

            for &idx in spawned_indices.iter().rev() {
                let pending = wave.spawn_queue.remove(idx);
                spawn_enemy_from_pending(
                    &mut commands,
                    &configs,
                    &diff_cfg,
                    &grid,
                    &mut wave,
                    &pending,
                );
            }

            if wave.spawn_queue.is_empty() {
                wave.wave_spawn_done = true;
            }
        }
    }
}

fn start_wave(
    wave_number: u32,
    configs: &GameConfigs,
    grid: &GridMap,
    wave: &mut WaveState,
    wave_started_writer: &mut MessageWriter<WaveStartedMsg>,
    sfx_writer: &mut MessageWriter<PlaySfxMsg>,
) {
    let wave_def = get_or_generate_wave(configs, wave_number);
    let spawn_count = grid.spawns.len().max(1);

    let mut queue = Vec::new();
    let mut spawn_rot = 0_usize;

    for group in &wave_def.groups {
        for i in 0..group.count {
            let delay = group.start_delay + i as f32 * group.spawn_interval;
            queue.push(PendingSpawn {
                archetype: group.archetype,
                delay_remaining: delay,
                spawn_index: spawn_rot % spawn_count,
                hp_override: group.hp_override,
                bounty_override: group.bounty_override,
                wave_number,
            });
            spawn_rot = spawn_rot.wrapping_add(1);
        }
    }

    wave.current_wave = wave_number;
    wave.phase = WavePhase::WaveActive;
    wave.prep_timer = 0.0;
    wave.spawn_queue = queue;
    wave.wave_spawn_done = wave.spawn_queue.is_empty();
    wave.active_boss = None;

    let next_preview = get_or_generate_wave(configs, wave_number + 1);
    wave.next_wave_has_air = next_preview.has_air;
    wave.next_wave_title = format!("Welle {}: {}", next_preview.wave_number, next_preview.title);
    wave.next_wave_is_boss = next_preview.is_boss_wave;

    wave_started_writer.write(WaveStartedMsg {
        wave: wave_number,
        is_boss_wave: wave_def.is_boss_wave,
        has_air: wave_def.has_air,
    });
    sfx_writer.write(PlaySfxMsg {
        kind: SfxEventKind::WaveStart,
        volume_scale: 1.0,
    });
}

fn spawn_enemy_from_pending(
    commands: &mut Commands,
    configs: &GameConfigs,
    diff_cfg: &DifficultyConfig,
    grid: &GridMap,
    wave: &mut WaveState,
    pending: &PendingSpawn,
) {
    let Some(enemy_cfg) = configs.get_enemy(pending.archetype) else {
        return;
    };

    let spawn_center = grid
        .spawns
        .get(pending.spawn_index)
        .or_else(|| grid.spawns.first())
        .map(|tiles| grid.zone_center_world(tiles))
        .unwrap_or(Vec3::ZERO);

    let jitter_x = (rand::random::<f32>() - 0.5) * 0.45;
    let jitter_z = (rand::random::<f32>() - 0.5) * 0.45;
    let y = if enemy_cfg.domain == MovementDomain::Air {
        AIR_ALTITUDE
    } else {
        0.0
    };
    let pos = Vec3::new(spawn_center.x + jitter_x, y, spawn_center.z + jitter_z);

    let (hp, armor, speed, bounty) = compute_scaled_enemy_stats(
        enemy_cfg,
        diff_cfg,
        &configs.waves,
        pending.wave_number,
        pending.hp_override,
        pending.bounty_override,
    );

    let entity = commands
        .spawn((
            GameplayEntity,
            Enemy {
                archetype: enemy_cfg.id,
                domain: enemy_cfg.domain,
                hp,
                max_hp: hp,
                base_speed: speed,
                base_armor: armor,
                slot_weight: enemy_cfg.slot_weight,
                bounty,
                bounty_paid: false,
                is_boss: enemy_cfg.is_boss,
                is_elite: enemy_cfg.is_elite,
                current_stage: 0,
                last_stage_dist: u16::MAX,
                reroute_count: 0,
                loop_count: 0,
                regen_pct_per_sec: enemy_cfg.regen_pct_per_sec,
                split_on_death: enemy_cfg.split_into,
                ability: enemy_cfg.boss_ability,
                ability_cooldown: 12.0,
                shield_timer: 0.0,
                anim_phase: rand::random::<f32>() * std::f32::consts::TAU,
                visual_spawned: false,
                scale: enemy_cfg.scale,
            },
            StatusEffects::default(),
            Transform::from_translation(pos),
            Visibility::default(),
        ))
        .id();

    if enemy_cfg.is_boss {
        wave.active_boss = Some(entity);
    }
}

fn compute_scaled_enemy_stats(
    enemy_cfg: &EnemyConfig,
    diff_cfg: &DifficultyConfig,
    waves_cfg: &WaveTableConfig,
    wave_number: u32,
    hp_override: Option<f32>,
    bounty_override: Option<u32>,
) -> (f32, f32, f32, u32) {
    let base_hp = hp_override.unwrap_or(enemy_cfg.base_hp);
    let base_bounty = bounty_override.unwrap_or(enemy_cfg.base_bounty);

    if wave_number <= 30 {
        let hp = (base_hp * diff_cfg.hp_mult).max(1.0);
        let armor = enemy_cfg.base_armor + diff_cfg.armor_add;
        let speed = enemy_cfg.base_speed;
        let bounty = ((base_bounty as f32) * diff_cfg.bounty_mult).round().max(1.0) as u32;
        (hp, armor, speed, bounty)
    } else {
        let extra = (wave_number - 30) as i32;
        let hp_scale = waves_cfg.endless_hp_growth.powi(extra);
        let bounty_scale = waves_cfg.endless_bounty_growth.powi(extra);
        let speed_mult = (1.0 + waves_cfg.endless_speed_per_wave * extra as f32)
            .min(waves_cfg.endless_speed_cap);

        let hp = (base_hp * hp_scale * diff_cfg.hp_mult).max(1.0);
        let armor =
            enemy_cfg.base_armor + waves_cfg.endless_armor_per_wave * extra as f32 + diff_cfg.armor_add;
        let speed = enemy_cfg.base_speed * speed_mult;
        let bounty = ((base_bounty as f32) * bounty_scale * diff_cfg.bounty_mult)
            .round()
            .max(1.0) as u32;
        (hp, armor, speed, bounty)
    }
}

#[allow(clippy::too_many_arguments)]
fn boss_abilities_system(
    mut commands: Commands,
    time: Res<Time>,
    speed: Res<GameSpeed>,
    configs: Res<GameConfigs>,
    match_cfg: Res<MatchConfig>,
    wave: Res<WaveState>,
    mut bosses: Query<(&mut Enemy, &Transform, &StatusEffects), Without<PendingDespawn>>,
    mut vfx_writer: MessageWriter<SpawnVfxMsg>,
    mut popup_writer: MessageWriter<SpawnPopupMsg>,
) {
    let dt = time.delta_secs() * speed.multiplier();
    if dt <= 0.0 || wave.phase != WavePhase::WaveActive {
        return;
    }

    let diff_cfg = configs.difficulty_cfg(match_cfg.difficulty);
    let mut summons: Vec<(EnemyArchetypeId, Vec3, usize, u16, u32)> = Vec::new();

    for (mut boss, tf, status) in &mut bosses {
        if !boss.is_boss || boss.hp <= 0.0 || boss.ability == BossAbilityKind::None {
            continue;
        }
        boss.ability_cooldown -= dt;
        if boss.ability_cooldown > 0.0 {
            continue;
        }
        boss.ability_cooldown = 13.5;

        match boss.ability {
            BossAbilityKind::SummonRats => {
                for i in 0..4 {
                    let angle = (i as f32 / 4.0) * std::f32::consts::TAU;
                    let pos = tf.translation + Vec3::new(angle.cos() * 0.5, 0.0, angle.sin() * 0.5);
                    summons.push((
                        EnemyArchetypeId::RatSwarm,
                        Vec3::new(pos.x, 0.0, pos.z),
                        boss.current_stage,
                        boss.last_stage_dist,
                        boss.loop_count,
                    ));
                }
                popup_writer.write(SpawnPopupMsg {
                    pos: tf.translation + Vec3::Y * 1.8,
                    text: "Ratten-Ruf!".into(),
                    color: Color::srgb(1.0, 0.72, 0.22),
                });
            }
            BossAbilityKind::BloodRegenPulse => {
                let heal_pct = 0.06 * (1.0 - status.anti_heal_fraction());
                let heal_amt = boss.max_hp * heal_pct;
                boss.hp = (boss.hp + heal_amt).min(boss.max_hp);
                popup_writer.write(SpawnPopupMsg {
                    pos: tf.translation + Vec3::Y * 1.8,
                    text: format!("+{} HP", heal_amt.round() as u32),
                    color: Color::srgb(0.95, 0.22, 0.28),
                });
                vfx_writer.write(SpawnVfxMsg {
                    kind: VfxKind::PoisonCloud,
                    pos: tf.translation + Vec3::Y * 0.8,
                    target_pos: None,
                    color: Color::srgb(0.95, 0.16, 0.24),
                    radius: 1.4,
                });
            }
            BossAbilityKind::StoneShield => {
                boss.shield_timer = 4.0;
                popup_writer.write(SpawnPopupMsg {
                    pos: tf.translation + Vec3::Y * 1.8,
                    text: "Steinschild!".into(),
                    color: Color::srgb(0.78, 0.82, 0.92),
                });
            }
            BossAbilityKind::SummonVampiresAndBats => {
                for i in 0..2 {
                    let angle = (i as f32 / 2.0) * std::f32::consts::TAU;
                    let off = Vec3::new(angle.cos() * 0.6, 0.0, angle.sin() * 0.6);
                    summons.push((
                        EnemyArchetypeId::VampireLord,
                        Vec3::new(tf.translation.x + off.x, 0.0, tf.translation.z + off.z),
                        boss.current_stage,
                        boss.last_stage_dist,
                        boss.loop_count,
                    ));
                    summons.push((
                        EnemyArchetypeId::BatFlyer,
                        Vec3::new(
                            tf.translation.x - off.x,
                            AIR_ALTITUDE,
                            tf.translation.z - off.z,
                        ),
                        boss.current_stage,
                        boss.last_stage_dist,
                        boss.loop_count,
                    ));
                }
                popup_writer.write(SpawnPopupMsg {
                    pos: tf.translation + Vec3::Y * 1.8,
                    text: "Dreadlord-Ruf!".into(),
                    color: Color::srgb(0.55, 0.95, 0.72),
                });
            }
            BossAbilityKind::SplitThree | BossAbilityKind::None => {}
        }
    }

    for (arch, pos, stage, stage_dist, loop_count) in summons {
        let Some(cfg) = configs.get_enemy(arch) else {
            continue;
        };
        let (hp, armor, move_spd, bounty) =
            compute_scaled_enemy_stats(cfg, &diff_cfg, &configs.waves, wave.current_wave, None, None);
        commands.spawn((
            GameplayEntity,
            Enemy {
                archetype: arch,
                domain: cfg.domain,
                hp,
                max_hp: hp,
                base_speed: move_spd,
                base_armor: armor,
                slot_weight: cfg.slot_weight,
                bounty,
                bounty_paid: false,
                is_boss: cfg.is_boss,
                is_elite: cfg.is_elite,
                current_stage: stage,
                last_stage_dist: stage_dist,
                reroute_count: 0,
                loop_count,
                regen_pct_per_sec: cfg.regen_pct_per_sec,
                split_on_death: cfg.split_into,
                ability: cfg.boss_ability,
                ability_cooldown: 12.0,
                shield_timer: 0.0,
                anim_phase: rand::random::<f32>() * std::f32::consts::TAU,
                visual_spawned: false,
                scale: cfg.scale,
            },
            StatusEffects::default(),
            Transform::from_translation(pos),
            Visibility::default(),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn enemy_movement_system(
    time: Res<Time>,
    speed: Res<GameSpeed>,
    configs: Res<GameConfigs>,
    grid: Res<GridMap>,
    flow: Res<FlowField>,
    mut wave: ResMut<WaveState>,
    mut enemies: Query<(Entity, &mut Enemy, &mut Transform, &StatusEffects), Without<PendingDespawn>>,
    mut leaked_writer: MessageWriter<EnemyLeakedMsg>,
    mut vfx_writer: MessageWriter<SpawnVfxMsg>,
    mut sfx_writer: MessageWriter<PlaySfxMsg>,
) {
    let dt = time.delta_secs() * speed.multiplier();
    if dt <= 0.0 || grid.width <= 0 || grid.height <= 0 {
        return;
    }

    let stage_count = grid.stage_count();
    let stage_centers: Vec<Vec3> = (0..stage_count)
        .map(|s| grid.zone_center_world(grid.stage_target_tiles(s)))
        .collect();
    let primary_spawn = grid
        .spawns
        .first()
        .map(|tiles| grid.zone_center_world(tiles))
        .unwrap_or(Vec3::ZERO);

    // Snapshot positions for lightweight boid separation (radius = 0.28)
    let positions: Vec<(Entity, MovementDomain, Vec3)> = enemies
        .iter()
        .filter(|(_, e, _, _)| e.hp > 0.0)
        .map(|(ent, e, tf, _)| (ent, e.domain, tf.translation))
        .collect();

    let sep_radius = 0.28_f32;
    let sep_radius_sq = sep_radius * sep_radius;
    let max_x = grid.width as f32 * 0.5 * TILE_SIZE - 0.15;
    let max_z = grid.height as f32 * 0.5 * TILE_SIZE - 0.15;

    for (entity, mut enemy, mut tf, status) in &mut enemies {
        if enemy.hp <= 0.0 {
            continue;
        }

        enemy.anim_phase += dt * 4.5;

        let speed_mult = status.speed_multiplier(
            enemy.is_boss,
            configs.balance.max_slow_normal,
            configs.balance.max_slow_boss,
        );
        let move_speed = enemy.base_speed * speed_mult;
        if move_speed <= 0.0 {
            continue;
        }

        let target_center = stage_centers
            .get(enemy.current_stage)
            .copied()
            .unwrap_or(primary_spawn);

        let mut desired_dir_xz = Vec2::ZERO;

        match enemy.domain {
            MovementDomain::Ground => {
                if let Some(coord) = grid.world_to_tile(tf.translation)
                    && let Some(tile_idx) = grid.idx(coord)
                    && let Some(stage) = flow.stages.get(enemy.current_stage)
                {
                    if let Some(&dist) = stage.dist.get(tile_idx) {
                        enemy.last_stage_dist = dist;
                    }
                    if let Some(&flow_dir) = stage.dir.get(tile_idx) {
                        desired_dir_xz = flow_dir;
                    }
                }
                if desired_dir_xz.length_squared() < 1e-4 {
                    let delta = Vec2::new(
                        target_center.x - tf.translation.x,
                        target_center.z - tf.translation.z,
                    );
                    desired_dir_xz = delta.normalize_or_zero();
                }

                // Mild boid separation so swarms don't collapse onto one pixel
                let mut sep = Vec2::ZERO;
                for &(other_ent, other_dom, other_pos) in &positions {
                    if other_ent == entity || other_dom != MovementDomain::Ground {
                        continue;
                    }
                    let dx = tf.translation.x - other_pos.x;
                    let dz = tf.translation.z - other_pos.z;
                    let d2 = dx * dx + dz * dz;
                    if d2 > 1e-5 && d2 < sep_radius_sq {
                        let d = d2.sqrt();
                        sep += Vec2::new(dx / d, dz / d) * ((sep_radius - d) / sep_radius);
                    }
                }
                desired_dir_xz = (desired_dir_xz + sep * 0.45).normalize_or_zero();
            }
            MovementDomain::Air => {
                let delta = Vec2::new(
                    target_center.x - tf.translation.x,
                    target_center.z - tf.translation.z,
                );
                desired_dir_xz = delta.normalize_or_zero();
                enemy.last_stage_dist = (delta.length() * 10.0).min(u16::MAX as f32) as u16;
                tf.translation.y = AIR_ALTITUDE + (enemy.anim_phase * 0.7).sin() * 0.14;
            }
        }

        let step = move_speed * dt;
        let next_x = (tf.translation.x + desired_dir_xz.x * step).clamp(-max_x, max_x);
        let next_z = (tf.translation.z + desired_dir_xz.y * step).clamp(-max_z, max_z);

        // Ground collision guard: do not step directly inside a tower tile
        if enemy.domain == MovementDomain::Ground {
            let cand_world = Vec3::new(next_x, 0.0, next_z);
            let blocked = grid
                .world_to_tile(cand_world)
                .and_then(|c| grid.get(c))
                .is_some_and(|k| !k.is_walkable());
            if !blocked {
                tf.translation.x = next_x;
                tf.translation.z = next_z;
            } else {
                // Slide along free axis toward tile center or checkpoint
                let x_only = Vec3::new(next_x, 0.0, tf.translation.z);
                if grid
                    .world_to_tile(x_only)
                    .and_then(|c| grid.get(c))
                    .is_some_and(|k| k.is_walkable())
                {
                    tf.translation.x = next_x;
                } else {
                    let z_only = Vec3::new(tf.translation.x, 0.0, next_z);
                    if grid
                        .world_to_tile(z_only)
                        .and_then(|c| grid.get(c))
                        .is_some_and(|k| k.is_walkable())
                    {
                        tf.translation.z = next_z;
                    }
                }
            }
        } else {
            tf.translation.x = next_x;
            tf.translation.z = next_z;
        }

        // Smoothly face movement direction
        if desired_dir_xz.length_squared() > 1e-4 {
            let yaw = desired_dir_xz.x.atan2(desired_dir_xz.y);
            let target_rot = Quat::from_rotation_y(yaw);
            tf.rotation = tf.rotation.slerp(target_rot, (dt * 10.0).min(1.0));
        }

        // Check checkpoint / Goal arrival
        let dist_xz = Vec2::new(
            target_center.x - tf.translation.x,
            target_center.z - tf.translation.z,
        )
        .length();

        if dist_xz <= 1.15 {
            enemy.current_stage += 1;
            enemy.reroute_count = 0;

            if enemy.current_stage >= stage_count {
                // WC3 Leak-Loop! Warp enemy back to Spawn, preserving HP and debuffs
                let spawn_pos = grid
                    .spawns
                    .get((enemy.loop_count as usize) % grid.spawns.len().max(1))
                    .map(|z| grid.zone_center_world(z))
                    .unwrap_or(primary_spawn);

                let warp_from = tf.translation;
                let y = if enemy.domain == MovementDomain::Air {
                    AIR_ALTITUDE
                } else {
                    0.0
                };
                tf.translation = Vec3::new(spawn_pos.x, y, spawn_pos.z);
                enemy.current_stage = 0;
                enemy.last_stage_dist = u16::MAX;
                enemy.loop_count += 1;
                wave.leaked_total += 1;

                leaked_writer.write(EnemyLeakedMsg {
                    enemy: entity,
                    pos: warp_from,
                    loop_count: enemy.loop_count,
                });
                vfx_writer.write(SpawnVfxMsg {
                    kind: VfxKind::PortalWarp,
                    pos: warp_from + Vec3::Y * 0.6,
                    target_pos: Some(tf.translation + Vec3::Y * 0.6),
                    color: Color::srgb(0.68, 0.25, 0.98),
                    radius: 1.0,
                });
                sfx_writer.write(PlaySfxMsg {
                    kind: SfxEventKind::PortalWarp,
                    volume_scale: 0.9,
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_wave_completion_system(
    configs: Res<GameConfigs>,
    match_cfg: Res<MatchConfig>,
    mut wave: ResMut<WaveState>,
    economy: Res<EconomyState>,
    enemies: Query<&Enemy, Without<PendingDespawn>>,
    mut completed_writer: MessageWriter<WaveCompletedMsg>,
    mut next_state: ResMut<NextState<AppState>>,
    mut sfx_writer: MessageWriter<PlaySfxMsg>,
    mut save_writer: MessageWriter<SavePersistenceMsg>,
) {
    if wave.phase != WavePhase::WaveActive || !wave.wave_spawn_done || !wave.spawn_queue.is_empty()
    {
        return;
    }

    let any_alive = enemies.iter().any(|e| e.hp > 0.0);
    if any_alive {
        return;
    }

    let cleared_wave = wave.current_wave;
    let wave_bonus =
        configs.balance.wave_bonus_base + configs.balance.wave_bonus_per_wave * cleared_wave;
    let interest_rate = configs.balance.interest_rate + economy.interest_rate_bonus;
    let interest_cap = configs.balance.interest_cap + economy.interest_cap_bonus;
    let interest = ((economy.gold as f32) * interest_rate)
        .floor()
        .min(interest_cap as f32)
        .max(0.0) as u32;

    completed_writer.write(WaveCompletedMsg {
        wave: cleared_wave,
        wave_bonus,
        interest,
    });

    if cleared_wave >= wave.max_standard_waves && !match_cfg.endless_active {
        wave.phase = WavePhase::BuildPrep;
        wave.prep_timer = configs.balance.prep_time_secs;
        next_state.set(AppState::Victory);
        sfx_writer.write(PlaySfxMsg {
            kind: SfxEventKind::VictoryFanfare,
            volume_scale: 1.0,
        });
        save_writer.write(SavePersistenceMsg);
    } else {
        wave.phase = WavePhase::BuildPrep;
        wave.prep_timer = configs.balance.prep_time_secs;
    }
}

pub fn get_or_generate_wave(configs: &GameConfigs, wave_number: u32) -> WaveDefConfig {
    if let Some(found) = configs
        .waves
        .waves
        .iter()
        .find(|w| w.wave_number == wave_number)
    {
        return found.clone();
    }

    // Deterministic Endless wave generator for wave > 30
    let w30_base_hp = 6200.0_f32;
    let is_boss = wave_number.is_multiple_of(5);
    let has_air = wave_number.is_multiple_of(2) || is_boss;

    let mut groups = vec![
        WaveGroupConfig {
            archetype: if wave_number.is_multiple_of(3) {
                EnemyArchetypeId::StoneGolem
            } else {
                EnemyArchetypeId::VampireLord
            },
            count: 16 + (wave_number % 8),
            hp_override: Some(w30_base_hp),
            bounty_override: Some(20),
            spawn_interval: 0.55,
            start_delay: 0.0,
        },
        WaveGroupConfig {
            archetype: EnemyArchetypeId::SlimeSplitter,
            count: 12,
            hp_override: Some(w30_base_hp * 0.75),
            bounty_override: Some(16),
            spawn_interval: 0.65,
            start_delay: 1.5,
        },
    ];

    if has_air {
        groups.push(WaveGroupConfig {
            archetype: EnemyArchetypeId::Gargoyle,
            count: 14,
            hp_override: Some(w30_base_hp * 0.90),
            bounty_override: Some(20),
            spawn_interval: 0.60,
            start_delay: 2.0,
        });
    }

    if is_boss {
        groups.push(WaveGroupConfig {
            archetype: if wave_number.is_multiple_of(10) {
                EnemyArchetypeId::BossAetherDreadlord
            } else {
                EnemyArchetypeId::BossGolemColossus
            },
            count: 1,
            hp_override: Some(58000.0),
            bounty_override: Some(160),
            spawn_interval: 1.0,
            start_delay: 1.0,
        });
    }

    WaveDefConfig {
        wave_number,
        title: if is_boss {
            format!("ENDLESS BOSS (Welle {wave_number})")
        } else if has_air {
            format!("Endless Belagerung + Luft (Welle {wave_number})")
        } else {
            format!("Endless Sturm (Welle {wave_number})")
        },
        is_boss_wave: is_boss,
        has_air,
        groups,
    }
}
