#![allow(clippy::type_complexity, clippy::too_many_arguments)]

use bevy::prelude::*;
use smallvec::SmallVec;

use crate::types::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                apply_passive_slow_auras,
                tower_attack_system,
                move_projectiles_system,
                tick_status_effects_and_hazards,
                resolve_enemy_deaths,
            )
                .chain()
                .in_set(GameSet::CombatSet)
                .run_if(in_state(AppState::Playing)),
        );
    }
}

#[derive(Clone)]
struct ShrineInfo {
    pos: Vec3,
    radius: f32,
    dmg_bonus: f32,
}

#[allow(clippy::too_many_arguments)]
fn apply_passive_slow_auras(
    configs: Res<GameConfigs>,
    towers: Query<(&Tower, &Transform), Without<PendingDespawn>>,
    mut enemies: Query<(&Enemy, &Transform, &mut StatusEffects), Without<PendingDespawn>>,
) {
    let mut auras: Vec<(Vec3, f32, f32)> = Vec::new();
    for (tower, tf) in &towers {
        if let Some(cfg) = configs.get_tower(tower.faction, &tower.id) {
            for eff in &cfg.on_hit_effects {
                if let OnHitEffect::SlowAura { radius, percent } = *eff {
                    auras.push((tf.translation, radius, percent));
                }
            }
        }
    }

    if auras.is_empty() {
        return;
    }

    for (enemy, enemy_tf, mut status) in &mut enemies {
        if enemy.hp <= 0.0 {
            continue;
        }
        for &(aura_pos, radius, percent) in &auras {
            let dx = enemy_tf.translation.x - aura_pos.x;
            let dz = enemy_tf.translation.z - aura_pos.z;
            if dx * dx + dz * dz <= radius * radius {
                if percent >= status.slow_pct || status.slow_timer <= 0.1 {
                    status.slow_pct = status.slow_pct.max(percent);
                }
                status.slow_timer = status.slow_timer.max(0.25);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn tower_attack_system(
    mut commands: Commands,
    time: Res<Time>,
    speed: Res<GameSpeed>,
    configs: Res<GameConfigs>,
    mut towers: Query<(Entity, &mut Tower, &Transform), Without<PendingDespawn>>,
    mut enemies: Query<
        (Entity, &mut Enemy, &Transform, &mut StatusEffects),
        Without<PendingDespawn>,
    >,
    mut damage_writer: MessageWriter<DamageAppliedMsg>,
    mut freeze_writer: MessageWriter<FreezeTriggeredMsg>,
    mut vfx_writer: MessageWriter<SpawnVfxMsg>,
    mut sfx_writer: MessageWriter<PlaySfxMsg>,
    mut popup_writer: MessageWriter<SpawnPopupMsg>,
) {
    let dt = time.delta_secs() * speed.multiplier();
    if dt <= 0.0 {
        return;
    }

    let mut shrines: Vec<ShrineInfo> = Vec::new();
    for (_, tower, tf) in &towers {
        if let Some(cfg) = configs.get_tower(tower.faction, &tower.id)
            && let AttackKind::SupportAura {
                aura_radius,
                dmg_bonus,
                ..
            } = cfg.attack_kind
            && dmg_bonus > 0.0
        {
            shrines.push(ShrineInfo {
                pos: tf.translation,
                radius: aura_radius,
                dmg_bonus,
            });
        }
    }

    let mut total_damage_delta: Vec<(Entity, f32)> = Vec::new();
    let mut shot_events: Vec<(Entity, AttackKind, TowerConfig, Vec3, Entity, Vec3, f32)> =
        Vec::new();

    for (tower_entity, mut tower, tower_tf) in &mut towers {
        tower.recoil = (tower.recoil - dt * 5.0).max(0.0);
        tower.cooldown = (tower.cooldown - dt).max(0.0);

        let Some(cfg) = configs.get_tower(tower.faction, &tower.id) else {
            continue;
        };

        if cfg.attack_speed <= 0.0 || cfg.damage <= 0.0 {
            continue;
        }

        let Some(target_entity) = tower.target else {
            tower.ramp_stacks = tower.ramp_stacks.saturating_sub(1);
            continue;
        };

        let Ok((_, target_enemy, target_tf, _)) = enemies.get(target_entity) else {
            tower.target = None;
            continue;
        };

        if target_enemy.hp <= 0.0 || !cfg.target_mask.can_target(target_enemy.domain) {
            tower.target = None;
            continue;
        }

        let tower_pos = tower_tf.translation;
        let dx = target_tf.translation.x - tower_pos.x;
        let dz = target_tf.translation.z - tower_pos.z;
        if dx * dx + dz * dz > cfg.range * cfg.range {
            tower.target = None;
            continue;
        }

        if tower.cooldown > 0.0 {
            continue;
        }

        let mut ramp_bonus = 0.0_f32;
        for eff in &cfg.on_hit_effects {
            if let OnHitEffect::RampAttackSpeed { max_bonus_aspd } = *eff {
                tower.ramp_stacks = tower.ramp_stacks.saturating_add(1).min(12);
                ramp_bonus = (tower.ramp_stacks as f32 / 12.0) * max_bonus_aspd;
            }
        }
        let effective_aspd = (cfg.attack_speed + ramp_bonus).max(0.1);
        tower.cooldown = 1.0 / effective_aspd;
        tower.recoil = 1.0;
        tower.shot_counter = tower.shot_counter.wrapping_add(1);

        let mut armor_pen_pct = 0.0_f32;
        for eff in &cfg.on_hit_effects {
            if let OnHitEffect::ArmorPen {
                every_nth,
                ignore_pct,
            } = *eff
                && every_nth > 0
                && (every_nth == 1 || tower.shot_counter.is_multiple_of(every_nth))
            {
                armor_pen_pct = armor_pen_pct.max(ignore_pct);
            }
        }

        shot_events.push((
            tower_entity,
            cfg.attack_kind.clone(),
            cfg.clone(),
            tower_pos,
            target_entity,
            target_tf.translation,
            armor_pen_pct,
        ));
    }

    for (
        tower_entity,
        attack_kind,
        cfg,
        tower_pos,
        target_entity,
        target_pos,
        armor_pen_pct,
    ) in shot_events
    {
        let faction = towers
            .get(tower_entity)
            .map(|(_, t, _)| t.faction)
            .unwrap_or_else(|_| tower_faction_from_cfg(&configs, &cfg.id).unwrap_or_default());
        let proj_color = Color::srgb(
            cfg.projectile_color.0,
            cfg.projectile_color.1,
            cfg.projectile_color.2,
        );
        let aura_dmg_bonus = compute_tower_aura_dmg_bonus(faction, tower_pos, &shrines);
        let muzzle_pos = tower_pos + Vec3::Y * 1.4;

        match attack_kind {
            AttackKind::Projectile {
                speed: proj_speed,
                splash_radius,
                is_heavy,
            } => {
                commands.spawn((
                    GameplayEntity,
                    Projectile {
                        source_tower: tower_entity,
                        faction,
                        target: Some(target_entity),
                        target_pos,
                        speed: proj_speed,
                        damage: cfg.damage,
                        splash_radius,
                        air_bonus_mult: cfg.air_bonus_mult,
                        armor_pen_pct,
                        on_hit: cfg.on_hit_effects.clone(),
                        bounce_remaining: 0,
                        hit_entities: SmallVec::new(),
                        color: proj_color,
                        is_heavy,
                        lifetime: 4.5,
                    },
                    Transform::from_translation(muzzle_pos),
                    Visibility::default(),
                ));
                sfx_writer.write(PlaySfxMsg {
                    kind: select_attack_sfx(&cfg),
                    volume_scale: 0.75,
                });
            }
            AttackKind::MultiShot {
                count,
                speed: proj_speed,
                bounce_once,
            } => {
                let mut targets: Vec<(Entity, Vec3)> = vec![(target_entity, target_pos)];
                let range_sq = cfg.range * cfg.range;
                for (e_ent, enemy, e_tf, _) in &enemies {
                    if targets.len() >= count as usize {
                        break;
                    }
                    if e_ent == target_entity
                        || enemy.hp <= 0.0
                        || !cfg.target_mask.can_target(enemy.domain)
                    {
                        continue;
                    }
                    let dx = e_tf.translation.x - tower_pos.x;
                    let dz = e_tf.translation.z - tower_pos.z;
                    if dx * dx + dz * dz <= range_sq {
                        targets.push((e_ent, e_tf.translation));
                    }
                }
                for (t_ent, t_pos) in targets {
                    commands.spawn((
                        GameplayEntity,
                        Projectile {
                            source_tower: tower_entity,
                            faction,
                            target: Some(t_ent),
                            target_pos: t_pos,
                            speed: proj_speed,
                            damage: cfg.damage,
                            splash_radius: 0.0,
                            air_bonus_mult: cfg.air_bonus_mult,
                            armor_pen_pct,
                            on_hit: cfg.on_hit_effects.clone(),
                            bounce_remaining: if bounce_once { 1 } else { 0 },
                            hit_entities: SmallVec::new(),
                            color: proj_color,
                            is_heavy: false,
                            lifetime: 4.5,
                        },
                        Transform::from_translation(muzzle_pos),
                        Visibility::default(),
                    ));
                }
                sfx_writer.write(PlaySfxMsg {
                    kind: select_attack_sfx(&cfg),
                    volume_scale: 0.75,
                });
            }
            AttackKind::ChainLightning {
                bounces,
                decay_per_bounce,
                jump_range,
            } => {
                let mut current_pos = muzzle_pos;
                let mut current_target = Some(target_entity);
                let mut hit_list: Vec<Entity> = Vec::with_capacity(bounces as usize);
                let jump_sq = jump_range * jump_range;

                for hop in 0..bounces {
                    let Some(hop_entity) = current_target else {
                        break;
                    };
                    let Ok((_, mut enemy, e_tf, mut status)) = enemies.get_mut(hop_entity) else {
                        break;
                    };
                    if enemy.hp <= 0.0 {
                        break;
                    }

                    let hop_pos = e_tf.translation;
                    let mult = (1.0 - decay_per_bounce).powi(hop as i32);
                    let raw_dmg = cfg.damage * mult;

                    let hit = apply_hit_to_enemy(
                        hop_entity,
                        &mut enemy,
                        &mut status,
                        hop_pos,
                        tower_entity,
                        raw_dmg,
                        cfg.air_bonus_mult,
                        armor_pen_pct,
                        aura_dmg_bonus,
                        &cfg.on_hit_effects,
                        &configs.balance,
                        &mut damage_writer,
                        &mut freeze_writer,
                        &mut vfx_writer,
                        &mut sfx_writer,
                        &mut popup_writer,
                    );
                    total_damage_delta.push((tower_entity, hit));
                    hit_list.push(hop_entity);

                    commands.spawn((
                        GameplayEntity,
                        LightningArcVfx {
                            start: current_pos,
                            end: hop_pos + Vec3::Y * 0.6,
                            color: proj_color,
                            timer: 0.16,
                            max_timer: 0.16,
                        },
                    ));
                    vfx_writer.write(SpawnVfxMsg {
                        kind: VfxKind::LightningArc,
                        pos: current_pos,
                        target_pos: Some(hop_pos + Vec3::Y * 0.6),
                        color: proj_color,
                        radius: 0.4,
                    });

                    current_pos = hop_pos + Vec3::Y * 0.6;
                    let mut next_best: Option<(Entity, f32)> = None;
                    for (cand_ent, cand_enemy, cand_tf, _) in &enemies {
                        if cand_enemy.hp <= 0.0
                            || hit_list.contains(&cand_ent)
                            || !cfg.target_mask.can_target(cand_enemy.domain)
                        {
                            continue;
                        }
                        let dx = cand_tf.translation.x - hop_pos.x;
                        let dz = cand_tf.translation.z - hop_pos.z;
                        let d2 = dx * dx + dz * dz;
                        if d2 <= jump_sq && next_best.is_none_or(|(_, best_d2)| d2 < best_d2) {
                            next_best = Some((cand_ent, d2));
                        }
                    }
                    current_target = next_best.map(|(e, _)| e);
                }

                sfx_writer.write(PlaySfxMsg {
                    kind: SfxEventKind::LightningZap,
                    volume_scale: 0.85,
                });
            }
            AttackKind::Beam { ramp_armor_shred } => {
                if let Ok((_, mut enemy, e_tf, mut status)) = enemies.get_mut(target_entity)
                    && enemy.hp > 0.0
                {
                    let e_pos = e_tf.translation;
                    let hit = apply_hit_to_enemy(
                        target_entity,
                        &mut enemy,
                        &mut status,
                        e_pos,
                        tower_entity,
                        cfg.damage,
                        cfg.air_bonus_mult,
                        armor_pen_pct,
                        aura_dmg_bonus,
                        &cfg.on_hit_effects,
                        &configs.balance,
                        &mut damage_writer,
                        &mut freeze_writer,
                        &mut vfx_writer,
                        &mut sfx_writer,
                        &mut popup_writer,
                    );
                    if ramp_armor_shred > 0.0 {
                        status.armor_shred = (status.armor_shred + ramp_armor_shred)
                            .min(configs.balance.max_armor_shred);
                        status.armor_shred_timer = status.armor_shred_timer.max(4.0);
                    }
                    total_damage_delta.push((tower_entity, hit));

                    commands.spawn((
                        GameplayEntity,
                        LightningArcVfx {
                            start: muzzle_pos,
                            end: e_pos + Vec3::Y * 0.6,
                            color: proj_color,
                            timer: 0.22,
                            max_timer: 0.22,
                        },
                    ));
                    vfx_writer.write(SpawnVfxMsg {
                        kind: VfxKind::LightningArc,
                        pos: muzzle_pos,
                        target_pos: Some(e_pos + Vec3::Y * 0.6),
                        color: proj_color,
                        radius: 0.35,
                    });
                    sfx_writer.write(PlaySfxMsg {
                        kind: SfxEventKind::PoisonCast,
                        volume_scale: 0.65,
                    });
                }
            }
            AttackKind::ConeAoe { radius } => {
                let rad_sq = radius * radius;
                for (e_ent, mut enemy, e_tf, mut status) in &mut enemies {
                    if enemy.hp <= 0.0 || !cfg.target_mask.can_target(enemy.domain) {
                        continue;
                    }
                    let dx = e_tf.translation.x - target_pos.x;
                    let dz = e_tf.translation.z - target_pos.z;
                    if dx * dx + dz * dz <= rad_sq {
                        let hit = apply_hit_to_enemy(
                            e_ent,
                            &mut enemy,
                            &mut status,
                            e_tf.translation,
                            tower_entity,
                            cfg.damage,
                            cfg.air_bonus_mult,
                            armor_pen_pct,
                            aura_dmg_bonus,
                            &cfg.on_hit_effects,
                            &configs.balance,
                            &mut damage_writer,
                            &mut freeze_writer,
                            &mut vfx_writer,
                            &mut sfx_writer,
                            &mut popup_writer,
                        );
                        total_damage_delta.push((tower_entity, hit));
                    }
                }
                let vfx_kind = match cfg.id.as_str() {
                    "steam_condenser" => VfxKind::SteamCloud,
                    "blizzard_obelisk" | "blizzard_spire" => VfxKind::IceShatter,
                    _ => VfxKind::FireExplosion,
                };
                vfx_writer.write(SpawnVfxMsg {
                    kind: vfx_kind,
                    pos: target_pos + Vec3::Y * 0.5,
                    target_pos: None,
                    color: proj_color,
                    radius,
                });
                sfx_writer.write(PlaySfxMsg {
                    kind: select_attack_sfx(&cfg),
                    volume_scale: 0.85,
                });
            }
            AttackKind::SupportAura { .. } => {
                // Support towers like Soul Crypt fire a basic bolt when damage > 0
                commands.spawn((
                    GameplayEntity,
                    Projectile {
                        source_tower: tower_entity,
                        faction,
                        target: Some(target_entity),
                        target_pos,
                        speed: 14.0,
                        damage: cfg.damage,
                        splash_radius: 0.0,
                        air_bonus_mult: cfg.air_bonus_mult,
                        armor_pen_pct,
                        on_hit: cfg.on_hit_effects.clone(),
                        bounce_remaining: 0,
                        hit_entities: SmallVec::new(),
                        color: proj_color,
                        is_heavy: false,
                        lifetime: 4.5,
                    },
                    Transform::from_translation(muzzle_pos),
                    Visibility::default(),
                ));
                sfx_writer.write(PlaySfxMsg {
                    kind: SfxEventKind::PoisonCast,
                    volume_scale: 0.6,
                });
            }
        }
    }

    for (tower_entity, dmg) in total_damage_delta {
        if let Ok((_, mut tower, _)) = towers.get_mut(tower_entity) {
            tower.total_damage += dmg;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn move_projectiles_system(
    mut commands: Commands,
    time: Res<Time>,
    speed: Res<GameSpeed>,
    configs: Res<GameConfigs>,
    mut projectiles: Query<(Entity, &mut Projectile, &mut Transform), Without<PendingDespawn>>,
    mut enemies: Query<
        (Entity, &mut Enemy, &Transform, &mut StatusEffects),
        (Without<PendingDespawn>, Without<Projectile>),
    >,
    mut towers: Query<(&mut Tower, &Transform), (Without<PendingDespawn>, Without<Projectile>)>,
    mut damage_writer: MessageWriter<DamageAppliedMsg>,
    mut freeze_writer: MessageWriter<FreezeTriggeredMsg>,
    mut vfx_writer: MessageWriter<SpawnVfxMsg>,
    mut sfx_writer: MessageWriter<PlaySfxMsg>,
    mut popup_writer: MessageWriter<SpawnPopupMsg>,
) {
    let dt = time.delta_secs() * speed.multiplier();
    if dt <= 0.0 {
        return;
    }

    let mut shrines: Vec<ShrineInfo> = Vec::new();
    for (tower, tf) in &towers {
        if let Some(cfg) = configs.get_tower(tower.faction, &tower.id)
            && let AttackKind::SupportAura {
                aura_radius,
                dmg_bonus,
                ..
            } = cfg.attack_kind
            && dmg_bonus > 0.0
        {
            shrines.push(ShrineInfo {
                pos: tf.translation,
                radius: aura_radius,
                dmg_bonus,
            });
        }
    }

    for (proj_entity, mut proj, mut proj_tf) in &mut projectiles {
        proj.lifetime -= dt;
        if proj.lifetime <= 0.0 {
            if let Ok(mut ec) = commands.get_entity(proj_entity) {
                ec.insert(PendingDespawn);
            }
            continue;
        }

        if let Some(target_ent) = proj.target
            && let Ok((_, enemy, e_tf, _)) = enemies.get(target_ent)
            && enemy.hp > 0.0
        {
            proj.target_pos = e_tf.translation + Vec3::Y * 0.5;
        }

        let to_target = proj.target_pos - proj_tf.translation;
        let dist = to_target.length();
        let step = proj.speed * dt;

        if dist > step.max(0.28) {
            proj_tf.translation += to_target.normalize_or_zero() * step;
            continue;
        }

        // Impact at target_pos
        proj_tf.translation = proj.target_pos;
        let impact_pos = proj.target_pos;

        let tower_pos = towers
            .get(proj.source_tower)
            .map(|(_, tf)| tf.translation)
            .unwrap_or(impact_pos);
        let aura_dmg_bonus = compute_tower_aura_dmg_bonus(proj.faction, tower_pos, &shrines);
        let target_mask = towers
            .get(proj.source_tower)
            .ok()
            .and_then(|(t, _)| configs.get_tower(t.faction, &t.id))
            .map(|c| c.target_mask)
            .unwrap_or(TargetMask::GroundAndAir);

        let mut total_hit_dmg = 0.0_f32;
        let mut primary_hit_entity: Option<Entity> = None;

        if proj.splash_radius > 0.0 {
            let rad_sq = proj.splash_radius * proj.splash_radius;
            for (e_ent, mut enemy, e_tf, mut status) in &mut enemies {
                if enemy.hp <= 0.0 || !target_mask.can_target(enemy.domain) {
                    continue;
                }
                let dx = e_tf.translation.x - impact_pos.x;
                let dz = e_tf.translation.z - impact_pos.z;
                if dx * dx + dz * dz <= rad_sq {
                    let is_primary = proj.target == Some(e_ent);
                    let falloff = if is_primary { 1.0 } else { 0.82 };
                    let hit = apply_hit_to_enemy(
                        e_ent,
                        &mut enemy,
                        &mut status,
                        e_tf.translation,
                        proj.source_tower,
                        proj.damage * falloff,
                        proj.air_bonus_mult,
                        proj.armor_pen_pct,
                        aura_dmg_bonus,
                        &proj.on_hit,
                        &configs.balance,
                        &mut damage_writer,
                        &mut freeze_writer,
                        &mut vfx_writer,
                        &mut sfx_writer,
                        &mut popup_writer,
                    );
                    total_hit_dmg += hit;
                    if is_primary {
                        primary_hit_entity = Some(e_ent);
                    }
                }
            }

            let vfx_kind = if proj
                .on_hit
                .iter()
                .any(|e| matches!(e, OnHitEffect::Burn { .. }))
                || proj.is_heavy
            {
                VfxKind::FireExplosion
            } else if proj
                .on_hit
                .iter()
                .any(|e| matches!(e, OnHitEffect::Poison { .. }))
            {
                VfxKind::PoisonCloud
            } else if proj
                .on_hit
                .iter()
                .any(|e| matches!(e, OnHitEffect::Slow { .. }))
            {
                VfxKind::IceShatter
            } else {
                VfxKind::ImpactSpark
            };

            vfx_writer.write(SpawnVfxMsg {
                kind: vfx_kind,
                pos: impact_pos,
                target_pos: None,
                color: proj.color,
                radius: proj.splash_radius,
            });
            if proj.is_heavy {
                sfx_writer.write(PlaySfxMsg {
                    kind: SfxEventKind::Explosion,
                    volume_scale: 0.75,
                });
            }
        } else if let Some(target_ent) = proj.target
            && let Ok((_, mut enemy, e_tf, mut status)) = enemies.get_mut(target_ent)
            && enemy.hp > 0.0
        {
            let hit = apply_hit_to_enemy(
                target_ent,
                &mut enemy,
                &mut status,
                e_tf.translation,
                proj.source_tower,
                proj.damage,
                proj.air_bonus_mult,
                proj.armor_pen_pct,
                aura_dmg_bonus,
                &proj.on_hit,
                &configs.balance,
                &mut damage_writer,
                &mut freeze_writer,
                &mut vfx_writer,
                &mut sfx_writer,
                &mut popup_writer,
            );
            total_hit_dmg += hit;
            primary_hit_entity = Some(target_ent);

            // AirKnockback effect (Northwind Tower)
            for eff in &proj.on_hit {
                if let OnHitEffect::AirKnockback { distance } = *eff
                    && enemy.domain == MovementDomain::Air
                    && let Ok(mut ec) = commands.get_entity(target_ent)
                {
                    let mut new_tf = *e_tf;
                    let push = (tower_pos - e_tf.translation).xz().normalize_or_zero()
                        * (distance * 0.6);
                    new_tf.translation.x -= push.x;
                    new_tf.translation.z -= push.y;
                    ec.insert(new_tf);
                }
            }

            vfx_writer.write(SpawnVfxMsg {
                kind: VfxKind::ImpactSpark,
                pos: impact_pos,
                target_pos: None,
                color: proj.color,
                radius: 0.35,
            });
        }

        if let Ok((mut tower, _)) = towers.get_mut(proj.source_tower) {
            tower.total_damage += total_hit_dmg;
        }

        if let Some(hit_ent) = primary_hit_entity {
            proj.hit_entities.push(hit_ent);
        }

        // Handle projectile bounce (Salven-Bastion / Volley Bastion)
        if proj.bounce_remaining > 0 {
            let mut next_bounce: Option<(Entity, Vec3, f32)> = None;
            let bounce_sq = 2.6 * 2.6;
            for (cand_ent, cand_enemy, cand_tf, _) in &enemies {
                if cand_enemy.hp <= 0.0
                    || proj.hit_entities.contains(&cand_ent)
                    || !target_mask.can_target(cand_enemy.domain)
                {
                    continue;
                }
                let dx = cand_tf.translation.x - impact_pos.x;
                let dz = cand_tf.translation.z - impact_pos.z;
                let d2 = dx * dx + dz * dz;
                if d2 <= bounce_sq && next_bounce.is_none_or(|(_, _, best_d2)| d2 < best_d2) {
                    next_bounce = Some((cand_ent, cand_tf.translation + Vec3::Y * 0.5, d2));
                }
            }
            if let Some((next_ent, next_pos, _)) = next_bounce {
                proj.bounce_remaining -= 1;
                proj.target = Some(next_ent);
                proj.target_pos = next_pos;
                proj.damage *= 0.75;
                continue;
            }
        }

        if let Ok(mut ec) = commands.get_entity(proj_entity) {
            ec.insert(PendingDespawn);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn tick_status_effects_and_hazards(
    mut commands: Commands,
    time: Res<Time>,
    speed: Res<GameSpeed>,
    mut enemies: Query<(Entity, &mut Enemy, &Transform, &mut StatusEffects), Without<PendingDespawn>>,
    mut hazards: Query<(Entity, &mut GroundHazard), Without<PendingDespawn>>,
    mut towers: Query<&mut Tower, Without<PendingDespawn>>,
    mut damage_writer: MessageWriter<DamageAppliedMsg>,
) {
    let dt = time.delta_secs() * speed.multiplier();
    if dt <= 0.0 {
        return;
    }

    let mut tower_dmg: Vec<(Entity, f32)> = Vec::new();

    // Tick GroundHazards (e.g. Pest Cathedral poison puddles)
    for (hazard_ent, mut hazard) in &mut hazards {
        hazard.timer -= dt;
        if hazard.timer <= 0.0 {
            if let Ok(mut ec) = commands.get_entity(hazard_ent) {
                ec.insert(PendingDespawn);
            }
            continue;
        }
        let rad_sq = hazard.radius * hazard.radius;
        for (_, enemy, e_tf, mut status) in &mut enemies {
            if enemy.hp <= 0.0 || enemy.domain != MovementDomain::Ground {
                continue;
            }
            let dx = e_tf.translation.x - hazard.pos.x;
            let dz = e_tf.translation.z - hazard.pos.z;
            if dx * dx + dz * dz <= rad_sq {
                if hazard.dps >= status.poison_dps {
                    status.poison_dps = hazard.dps;
                    status.poison_source = Some(hazard.source_tower);
                }
                status.poison_anti_heal = status.poison_anti_heal.max(hazard.anti_heal);
                status.poison_timer = status.poison_timer.max(0.6);
            }
        }
    }

    // Tick enemy status timers, DoTs, and anti-heal regeneration
    for (enemy_ent, mut enemy, e_tf, mut status) in &mut enemies {
        if enemy.hp <= 0.0 {
            continue;
        }

        if enemy.shield_timer > 0.0 {
            enemy.shield_timer = (enemy.shield_timer - dt).max(0.0);
        }

        // Burn DoT
        if status.burn_timer > 0.0 {
            status.burn_timer = (status.burn_timer - dt).max(0.0);
            let dmg = (status.burn_dps * dt).max(0.0);
            if dmg > 0.0 {
                enemy.hp -= dmg;
                if let Some(src) = status.burn_source {
                    tower_dmg.push((src, dmg));
                }
                damage_writer.write(DamageAppliedMsg {
                    enemy: enemy_ent,
                    source_tower: status.burn_source,
                    amount: dmg,
                    is_crit: false,
                    is_shatter: false,
                    pos: e_tf.translation,
                });
            }
            if status.burn_timer <= 0.0 {
                status.burn_dps = 0.0;
                status.burn_anti_heal = 0.0;
                status.burn_spread = false;
            }
        }

        // Poison DoT
        if status.poison_timer > 0.0 && enemy.hp > 0.0 {
            status.poison_timer = (status.poison_timer - dt).max(0.0);
            let dmg = (status.poison_dps * dt).max(0.0);
            if dmg > 0.0 {
                enemy.hp -= dmg;
                if let Some(src) = status.poison_source {
                    tower_dmg.push((src, dmg));
                }
                damage_writer.write(DamageAppliedMsg {
                    enemy: enemy_ent,
                    source_tower: status.poison_source,
                    amount: dmg,
                    is_crit: false,
                    is_shatter: false,
                    pos: e_tf.translation,
                });
            }
            if status.poison_timer <= 0.0 {
                status.poison_dps = 0.0;
                status.poison_stacks = 0;
                status.poison_anti_heal = 0.0;
                status.poison_puddle = false;
            }
        }

        // Regeneration with Anti-Heal reduction
        if enemy.hp > 0.0 && enemy.regen_pct_per_sec > 0.0 && enemy.hp < enemy.max_hp {
            let heal = compute_effective_regen(
                enemy.max_hp,
                enemy.regen_pct_per_sec,
                status.anti_heal_fraction(),
                dt,
            );
            enemy.hp = (enemy.hp + heal).min(enemy.max_hp);
        }

        // Remaining debuff durations
        if status.slow_timer > 0.0 {
            status.slow_timer = (status.slow_timer - dt).max(0.0);
            if status.slow_timer <= 0.0 {
                status.slow_pct = 0.0;
                status.frost_stacks = 0;
            }
        }
        if status.freeze_timer > 0.0 {
            status.freeze_timer = (status.freeze_timer - dt).max(0.0);
        }
        if status.freeze_immunity > 0.0 {
            status.freeze_immunity = (status.freeze_immunity - dt).max(0.0);
        }
        if status.armor_shred_timer > 0.0 {
            status.armor_shred_timer = (status.armor_shred_timer - dt).max(0.0);
            if status.armor_shred_timer <= 0.0 {
                status.armor_shred = 0.0;
            }
        }
        if status.stun_timer > 0.0 {
            status.stun_timer = (status.stun_timer - dt).max(0.0);
        }
        if status.stun_immunity > 0.0 {
            status.stun_immunity = (status.stun_immunity - dt).max(0.0);
        }
        if status.curse_timer > 0.0 {
            status.curse_timer = (status.curse_timer - dt).max(0.0);
            if status.curse_timer <= 0.0 {
                status.curse_amp = 0.0;
            }
        }
        if status.enrage_timer > 0.0 {
            status.enrage_timer = (status.enrage_timer - dt).max(0.0);
        }
        if status.enrage_cooldown > 0.0 {
            status.enrage_cooldown = (status.enrage_cooldown - dt).max(0.0);
        }
    }

    for (tower_ent, dmg) in tower_dmg {
        if let Ok(mut tower) = towers.get_mut(tower_ent) {
            tower.total_damage += dmg;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_enemy_deaths(
    mut commands: Commands,
    configs: Res<GameConfigs>,
    match_cfg: Res<MatchConfig>,
    mut wave: ResMut<WaveState>,
    mut enemies: Query<(Entity, &mut Enemy, &Transform, &mut StatusEffects), Without<PendingDespawn>>,
    mut towers: Query<(Entity, &mut Tower), Without<PendingDespawn>>,
    mut killed_writer: MessageWriter<EnemyKilledMsg>,
    mut damage_writer: MessageWriter<DamageAppliedMsg>,
    mut vfx_writer: MessageWriter<SpawnVfxMsg>,
    mut popup_writer: MessageWriter<SpawnPopupMsg>,
) {
    struct DeathSnapshot {
        entity: Entity,
        archetype: EnemyArchetypeId,
        pos: Vec3,
        max_hp: f32,
        bounty: u32,
        is_boss: bool,
        is_elite: bool,
        current_stage: usize,
        last_stage_dist: u16,
        loop_count: u32,
        split_on_death: Option<(EnemyArchetypeId, u8, f32)>,
        was_burning: bool,
        was_slowed_or_frozen: bool,
        burn_spread: bool,
        burn_dps: f32,
        burn_anti_heal: f32,
        burn_source: Option<Entity>,
        poison_puddle: bool,
        poison_dps: f32,
        poison_anti_heal: f32,
        poison_source: Option<Entity>,
        killer_tower: Option<Entity>,
        shatter_nova_radius: f32,
        death_explosion: Option<(f32, f32)>,
    }

    let mut deaths: Vec<DeathSnapshot> = Vec::new();

    for (entity, mut enemy, tf, status) in &mut enemies {
        if enemy.hp > 0.0 || enemy.bounty_paid {
            continue;
        }
        enemy.hp = 0.0;
        enemy.bounty_paid = true;

        if let Ok(mut ec) = commands.get_entity(entity) {
            ec.insert(PendingDespawn);
        }

        if wave.active_boss == Some(entity) {
            wave.active_boss = None;
        }

        let mut targeting_tower: Option<Entity> = None;
        let mut shatter_nova_radius = 0.0_f32;
        let mut death_explosion: Option<(f32, f32)> = None;

        for (tower_ent, tower) in &towers {
            if tower.target == Some(entity) {
                if targeting_tower.is_none() {
                    targeting_tower = Some(tower_ent);
                }
                if let Some(cfg) = configs.get_tower(tower.faction, &tower.id) {
                    for eff in &cfg.on_hit_effects {
                        match *eff {
                            OnHitEffect::DeathExplosion { max_hp_pct, radius } => {
                                death_explosion = Some((max_hp_pct, radius));
                            }
                            OnHitEffect::Shatter { nova_radius, .. }
                                if status.is_frozen() && nova_radius > 0.0 =>
                            {
                                shatter_nova_radius = shatter_nova_radius.max(nova_radius);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let killer_tower = status.burn_source.or(status.poison_source).or(targeting_tower);

        deaths.push(DeathSnapshot {
            entity,
            archetype: enemy.archetype,
            pos: tf.translation,
            max_hp: enemy.max_hp,
            bounty: enemy.bounty,
            is_boss: enemy.is_boss,
            is_elite: enemy.is_elite,
            current_stage: enemy.current_stage,
            last_stage_dist: enemy.last_stage_dist,
            loop_count: enemy.loop_count,
            split_on_death: enemy.split_on_death,
            was_burning: status.is_burning(),
            was_slowed_or_frozen: status.is_slowed_or_frozen(),
            burn_spread: status.burn_spread,
            burn_dps: status.burn_dps,
            burn_anti_heal: status.burn_anti_heal,
            burn_source: status.burn_source,
            poison_puddle: status.poison_puddle,
            poison_dps: status.poison_dps,
            poison_anti_heal: status.poison_anti_heal,
            poison_source: status.poison_source,
            killer_tower,
            shatter_nova_radius,
            death_explosion,
        });
    }

    if deaths.is_empty() {
        return;
    }

    let diff_cfg = configs.difficulty_cfg(match_cfg.difficulty);

    for d in deaths {
        killed_writer.write(EnemyKilledMsg {
            enemy: d.entity,
            archetype: d.archetype,
            pos: d.pos,
            bounty: d.bounty,
            is_boss: d.is_boss,
            is_elite: d.is_elite,
            was_burning: d.was_burning,
            was_slowed_or_frozen: d.was_slowed_or_frozen,
            source_tower: d.killer_tower,
        });

        if let Some(src_tower) = d.killer_tower
            && let Ok((_, mut t)) = towers.get_mut(src_tower)
        {
            t.kills += 1;
        }

        vfx_writer.write(SpawnVfxMsg {
            kind: VfxKind::EnemyDeath,
            pos: d.pos + Vec3::Y * 0.4,
            target_pos: None,
            color: Color::srgb(0.95, 0.72, 0.28),
            radius: if d.is_boss { 1.6 } else { 0.8 },
        });

        // Throttled gold popup for bosses, elites, or high-bounty kills
        if d.bounty >= 6 || d.is_boss || d.is_elite {
            popup_writer.write(SpawnPopupMsg {
                pos: d.pos + Vec3::Y * 1.1,
                text: format!("+{}g", d.bounty),
                color: Color::srgb(1.0, 0.88, 0.25),
            });
        }

        // DeathExplosion (Plasma Core)
        if let Some((max_hp_pct, radius)) = d.death_explosion {
            let exp_dmg = (d.max_hp * max_hp_pct).max(1.0);
            let rad_sq = radius * radius;
            for (e_ent, mut e, e_tf, _) in &mut enemies {
                if e.hp <= 0.0 || e_ent == d.entity {
                    continue;
                }
                let dx = e_tf.translation.x - d.pos.x;
                let dz = e_tf.translation.z - d.pos.z;
                if dx * dx + dz * dz <= rad_sq {
                    let final_dmg = (exp_dmg * wc3_armor_multiplier(e.base_armor)).max(1.0);
                    e.hp -= final_dmg;
                    damage_writer.write(DamageAppliedMsg {
                        enemy: e_ent,
                        source_tower: None,
                        amount: final_dmg,
                        is_crit: true,
                        is_shatter: false,
                        pos: e_tf.translation,
                    });
                }
            }
            vfx_writer.write(SpawnVfxMsg {
                kind: VfxKind::FireExplosion,
                pos: d.pos + Vec3::Y * 0.5,
                target_pos: None,
                color: Color::srgb(0.98, 0.35, 0.95),
                radius,
            });
        }

        // Elder Fire Dragon burn spread on death
        if d.burn_spread && d.burn_dps > 0.0 {
            let spread_sq = 2.4 * 2.4;
            for (e_ent, e, e_tf, mut st) in &mut enemies {
                if e.hp <= 0.0 || e_ent == d.entity {
                    continue;
                }
                let dx = e_tf.translation.x - d.pos.x;
                let dz = e_tf.translation.z - d.pos.z;
                if dx * dx + dz * dz <= spread_sq {
                    if d.burn_dps >= st.burn_dps {
                        st.burn_dps = d.burn_dps;
                        st.burn_source = d.burn_source;
                    }
                    st.burn_anti_heal = st.burn_anti_heal.max(d.burn_anti_heal);
                    st.burn_timer = st.burn_timer.max(3.5);
                }
            }
        }

        // Pest Cathedral poison puddle on death
        if d.poison_puddle
            && let Some(src) = d.poison_source
        {
            commands.spawn((
                GameplayEntity,
                GroundHazard {
                    pos: Vec3::new(d.pos.x, 0.05, d.pos.z),
                    radius: 1.8,
                    dps: d.poison_dps.max(18.0),
                    anti_heal: d.poison_anti_heal.max(0.90),
                    timer: 3.0,
                    source_tower: src,
                    faction: FactionId::Necropolis,
                },
            ));
            vfx_writer.write(SpawnVfxMsg {
                kind: VfxKind::PoisonCloud,
                pos: Vec3::new(d.pos.x, 0.2, d.pos.z),
                target_pos: None,
                color: Color::srgb(0.28, 1.0, 0.18),
                radius: 1.8,
            });
        }

        // Glacier Sniper Shatter Nova on death
        if d.shatter_nova_radius > 0.0 {
            let rad_sq = d.shatter_nova_radius * d.shatter_nova_radius;
            for (e_ent, e, e_tf, mut st) in &mut enemies {
                if e.hp <= 0.0 || e_ent == d.entity {
                    continue;
                }
                let dx = e_tf.translation.x - d.pos.x;
                let dz = e_tf.translation.z - d.pos.z;
                if dx * dx + dz * dz <= rad_sq {
                    st.slow_pct = st.slow_pct.max(0.45);
                    st.slow_timer = st.slow_timer.max(3.0);
                    st.frost_stacks = (st.frost_stacks + 2).min(5);
                }
            }
            vfx_writer.write(SpawnVfxMsg {
                kind: VfxKind::IceShatter,
                pos: d.pos + Vec3::Y * 0.5,
                target_pos: None,
                color: Color::srgb(0.65, 0.95, 1.0),
                radius: d.shatter_nova_radius,
            });
        }

        // Split on death (SlimeSplitter -> 2 MiniSlime; BossSlimeTitan -> 3 SlimeSplitter)
        if let Some((split_arch, count, hp_ratio)) = d.split_on_death
            && let Some(split_cfg) = configs.get_enemy(split_arch)
        {
            let child_hp = (d.max_hp * hp_ratio).max(split_cfg.base_hp * diff_cfg.hp_mult * 0.35);
            let child_bounty = ((split_cfg.base_bounty as f32) * diff_cfg.bounty_mult)
                .round()
                .max(1.0) as u32;
            for i in 0..count {
                let angle = (i as f32 / count.max(1) as f32) * std::f32::consts::TAU;
                let offset = Vec3::new(angle.cos() * 0.35, 0.0, angle.sin() * 0.35);
                let spawn_pos = Vec3::new(d.pos.x + offset.x, 0.0, d.pos.z + offset.z);
                commands.spawn((
                    GameplayEntity,
                    Enemy {
                        archetype: split_arch,
                        domain: split_cfg.domain,
                        hp: child_hp,
                        max_hp: child_hp,
                        base_speed: split_cfg.base_speed,
                        base_armor: split_cfg.base_armor + diff_cfg.armor_add,
                        slot_weight: split_cfg.slot_weight,
                        bounty: child_bounty,
                        bounty_paid: false,
                        is_boss: split_cfg.is_boss,
                        is_elite: split_cfg.is_elite,
                        current_stage: d.current_stage,
                        last_stage_dist: d.last_stage_dist,
                        reroute_count: 0,
                        loop_count: d.loop_count,
                        regen_pct_per_sec: split_cfg.regen_pct_per_sec,
                        split_on_death: split_cfg.split_into,
                        ability: split_cfg.boss_ability,
                        ability_cooldown: 12.0,
                        shield_timer: 0.0,
                        anim_phase: i as f32 * 1.4,
                        visual_spawned: false,
                        scale: split_cfg.scale,
                    },
                    StatusEffects::default(),
                    Transform::from_translation(spawn_pos),
                    Visibility::default(),
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_hit_to_enemy(
    enemy_entity: Entity,
    enemy: &mut Enemy,
    status: &mut StatusEffects,
    enemy_pos: Vec3,
    source_tower: Entity,
    base_damage: f32,
    air_bonus_mult: f32,
    armor_pen_pct: f32,
    aura_dmg_bonus: f32,
    on_hit_effects: &[OnHitEffect],
    balance: &BalanceConfig,
    damage_writer: &mut MessageWriter<DamageAppliedMsg>,
    freeze_writer: &mut MessageWriter<FreezeTriggeredMsg>,
    vfx_writer: &mut MessageWriter<SpawnVfxMsg>,
    sfx_writer: &mut MessageWriter<PlaySfxMsg>,
    popup_writer: &mut MessageWriter<SpawnPopupMsg>,
) -> f32 {
    let mut is_crit = false;
    let mut is_shatter = false;
    let mut crit_mult = 1.0_f32;
    let mut shatter_bonus = 0.0_f32;

    for eff in on_hit_effects {
        match *eff {
            OnHitEffect::Crit { chance, mult } if rand::random::<f32>() < chance => {
                is_crit = true;
                crit_mult = crit_mult.max(mult);
            }
            OnHitEffect::Shatter { bonus_mult, .. } if status.is_frozen() => {
                is_shatter = true;
                shatter_bonus = shatter_bonus.max(bonus_mult);
            }
            _ => {}
        }
    }

    let final_dmg = compute_wc3_damage(
        base_damage,
        enemy.domain,
        enemy.base_armor,
        enemy.shield_timer > 0.0,
        air_bonus_mult,
        armor_pen_pct,
        status.armor_shred,
        status.curse_amp,
        aura_dmg_bonus,
        shatter_bonus,
        crit_mult,
    );

    enemy.hp -= final_dmg;

    // Apply on-hit status effects
    for eff in on_hit_effects {
        match *eff {
            OnHitEffect::Burn {
                dps,
                duration,
                anti_heal,
                spread_on_death,
            } => {
                if dps >= status.burn_dps || status.burn_timer <= 0.0 {
                    status.burn_dps = dps;
                    status.burn_source = Some(source_tower);
                }
                status.burn_timer = status.burn_timer.max(duration);
                status.burn_anti_heal = status.burn_anti_heal.max(anti_heal);
                status.burn_spread |= spread_on_death;
            }
            OnHitEffect::Poison {
                dps,
                duration,
                anti_heal,
                puddle_on_death,
            } => {
                if status.poison_timer <= 0.0 {
                    status.poison_dps = dps;
                    status.poison_stacks = 1;
                    status.poison_source = Some(source_tower);
                } else if status.poison_stacks < 2 {
                    status.poison_dps += dps * 0.75;
                    status.poison_stacks += 1;
                    status.poison_source = Some(source_tower);
                } else if dps > status.poison_dps {
                    status.poison_dps = dps;
                    status.poison_source = Some(source_tower);
                }
                status.poison_timer = status.poison_timer.max(duration);
                status.poison_anti_heal = status.poison_anti_heal.max(anti_heal);
                status.poison_puddle |= puddle_on_death;
            }
            OnHitEffect::Slow {
                percent,
                duration,
                frost_stacks,
            } => {
                if percent >= status.slow_pct || status.slow_timer <= 0.0 {
                    status.slow_pct = status.slow_pct.max(percent);
                }
                status.slow_timer = status.slow_timer.max(duration);
                if frost_stacks > 0
                    && status.freeze_timer <= 0.0
                    && status.freeze_immunity <= 0.0
                {
                    let triggered = apply_frost_stacks(status, frost_stacks, 1.2);
                    if triggered {
                        if !status.freeze_rewarded {
                            status.freeze_rewarded = true;
                            freeze_writer.write(FreezeTriggeredMsg {
                                enemy: enemy_entity,
                                pos: enemy_pos,
                            });
                        }
                        vfx_writer.write(SpawnVfxMsg {
                            kind: VfxKind::IceShatter,
                            pos: enemy_pos + Vec3::Y * 0.6,
                            target_pos: None,
                            color: Color::srgb(0.60, 0.94, 1.0),
                            radius: 0.9,
                        });
                        sfx_writer.write(PlaySfxMsg {
                            kind: SfxEventKind::Freeze,
                            volume_scale: 0.85,
                        });
                    }
                }
            }
            OnHitEffect::FreezeChance { chance, duration }
                if status.freeze_timer <= 0.0
                    && status.freeze_immunity <= 0.0
                    && rand::random::<f32>() < chance =>
            {
                status.freeze_timer = duration;
                status.freeze_immunity = 3.0;
                status.frost_stacks = 0;
                if !status.freeze_rewarded {
                    status.freeze_rewarded = true;
                    freeze_writer.write(FreezeTriggeredMsg {
                        enemy: enemy_entity,
                        pos: enemy_pos,
                    });
                }
                vfx_writer.write(SpawnVfxMsg {
                    kind: VfxKind::IceShatter,
                    pos: enemy_pos + Vec3::Y * 0.6,
                    target_pos: None,
                    color: Color::srgb(0.60, 0.94, 1.0),
                    radius: 0.9,
                });
                sfx_writer.write(PlaySfxMsg {
                    kind: SfxEventKind::Freeze,
                    volume_scale: 0.85,
                });
            }
            OnHitEffect::ArmorShred {
                amount,
                max_stack,
                duration,
            } => {
                let cap = max_stack.min(balance.max_armor_shred);
                if amount < max_stack {
                    status.armor_shred = (status.armor_shred + amount).min(cap);
                } else {
                    status.armor_shred = status.armor_shred.max(cap);
                }
                status.armor_shred_timer = status.armor_shred_timer.max(duration);
            }
            OnHitEffect::MiniStun { duration, immunity }
                if !enemy.is_boss && status.stun_immunity <= 0.0 =>
            {
                status.stun_timer = status.stun_timer.max(duration);
                status.stun_immunity = immunity;
            }
            OnHitEffect::Curse { dmg_amp, duration } => {
                status.curse_amp = status.curse_amp.max(dmg_amp);
                status.curse_timer = status.curse_timer.max(duration);
            }
            _ => {}
        }
    }

    damage_writer.write(DamageAppliedMsg {
        enemy: enemy_entity,
        source_tower: Some(source_tower),
        amount: final_dmg,
        is_crit,
        is_shatter,
        pos: enemy_pos,
    });

    if is_shatter {
        popup_writer.write(SpawnPopupMsg {
            pos: enemy_pos + Vec3::Y * 1.25,
            text: format!("SHATTER {}!", final_dmg.round() as u32),
            color: Color::srgb(0.48, 0.95, 1.0),
        });
    } else if is_crit {
        popup_writer.write(SpawnPopupMsg {
            pos: enemy_pos + Vec3::Y * 1.15,
            text: format!("{}!", final_dmg.round() as u32),
            color: Color::srgb(1.0, 0.42, 0.20),
        });
    }

    final_dmg
}

#[allow(clippy::too_many_arguments)]
pub fn compute_wc3_damage(
    base_damage: f32,
    domain: MovementDomain,
    base_armor: f32,
    shield_active: bool,
    air_bonus_mult: f32,
    armor_pen_pct: f32,
    armor_shred: f32,
    curse_amp: f32,
    aura_dmg_bonus: f32,
    shatter_bonus: f32,
    crit_mult: f32,
) -> f32 {
    let air_mult = if domain == MovementDomain::Air {
        1.0 + air_bonus_mult
    } else {
        1.0
    };
    let shatter_mult = 1.0 + shatter_bonus;
    let amp_mult = 1.0 + curse_amp + aura_dmg_bonus;
    let shield_mult = if shield_active { 0.50 } else { 1.0 };

    let raw = base_damage * air_mult * shatter_mult * crit_mult * amp_mult * shield_mult;
    let eff_armor = (base_armor * (1.0 - armor_pen_pct.clamp(0.0, 1.0)) - armor_shred)
        .clamp(-45.0, 300.0);
    (raw * wc3_armor_multiplier(eff_armor)).max(1.0)
}

pub fn apply_frost_stacks(status: &mut StatusEffects, stacks: u8, freeze_duration: f32) -> bool {
    if status.freeze_timer > 0.0 || status.freeze_immunity > 0.0 {
        return false;
    }
    status.frost_stacks = status.frost_stacks.saturating_add(stacks);
    if status.frost_stacks >= 5 {
        status.frost_stacks = 0;
        status.freeze_timer = freeze_duration;
        status.freeze_immunity = 3.0;
        true
    } else {
        false
    }
}

pub fn compute_effective_regen(
    max_hp: f32,
    regen_pct_per_sec: f32,
    anti_heal_fraction: f32,
    dt: f32,
) -> f32 {
    max_hp * regen_pct_per_sec * (1.0 - anti_heal_fraction.clamp(0.0, 0.95)) * dt
}

fn compute_tower_aura_dmg_bonus(
    faction: FactionId,
    tower_pos: Vec3,
    shrines: &[ShrineInfo],
) -> f32 {
    if faction != FactionId::DragonClan {
        return 0.0;
    }
    let mut best = 0.0_f32;
    for s in shrines {
        let dx = tower_pos.x - s.pos.x;
        let dz = tower_pos.z - s.pos.z;
        if dx * dx + dz * dz <= s.radius * s.radius {
            best = best.max(s.dmg_bonus);
        }
    }
    best
}

fn tower_faction_from_cfg(configs: &GameConfigs, tower_id: &str) -> Option<FactionId> {
    for (fid, fcfg) in &configs.factions {
        if fcfg.towers.contains_key(tower_id) {
            return Some(*fid);
        }
    }
    None
}

fn select_attack_sfx(cfg: &TowerConfig) -> SfxEventKind {
    for eff in &cfg.on_hit_effects {
        match eff {
            OnHitEffect::Burn { .. } => return SfxEventKind::FireCast,
            OnHitEffect::Poison { .. } => return SfxEventKind::PoisonCast,
            OnHitEffect::Slow { .. } | OnHitEffect::Shatter { .. } => {
                return SfxEventKind::IceHit;
            }
            _ => {}
        }
    }
    match cfg.attack_kind {
        AttackKind::ChainLightning { .. } => SfxEventKind::LightningZap,
        AttackKind::Projectile { is_heavy: true, .. } => SfxEventKind::Explosion,
        AttackKind::ConeAoe { .. } => SfxEventKind::FireCast,
        _ => SfxEventKind::ArrowShot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wc3_armor_multiplier_positive_zero_negative() {
        let zero = wc3_armor_multiplier(0.0);
        assert!((zero - 1.0).abs() < 1e-5);

        let armor_50 = wc3_armor_multiplier(50.0);
        assert!((armor_50 - (100.0 / 150.0)).abs() < 1e-5);

        let armor_100 = wc3_armor_multiplier(100.0);
        assert!((armor_100 - 0.5).abs() < 1e-5);

        let neg_25 = wc3_armor_multiplier(-25.0);
        assert!((neg_25 - 1.2).abs() < 1e-5);

        let neg_45 = wc3_armor_multiplier(-45.0);
        assert!((neg_45 - (2.0 - 100.0 / 145.0)).abs() < 1e-5);
    }

    #[test]
    fn test_anti_heal_math_on_vampire_regen() {
        let mut status = StatusEffects {
            poison_timer: 4.0,
            poison_anti_heal: 0.95,
            burn_timer: 2.0,
            burn_anti_heal: 0.70,
            ..default()
        };
        let anti_heal = status.anti_heal_fraction();
        assert!((anti_heal - 0.95).abs() < 1e-5);

        let regen_no_debuff = compute_effective_regen(1000.0, 0.045, 0.0, 1.0);
        assert!((regen_no_debuff - 45.0).abs() < 1e-4);

        let regen_with_pest = compute_effective_regen(1000.0, 0.045, anti_heal, 1.0);
        assert!((regen_with_pest - 2.25).abs() < 1e-4);

        // Even if someone passes >0.95, clamp prevents 100% heal-lock beyond 95%
        status.poison_anti_heal = 1.0;
        assert!((status.anti_heal_fraction() - 0.95).abs() < 1e-5);
    }

    #[test]
    fn test_frost_stacks_trigger_freeze_and_immunity() {
        let mut status = StatusEffects::default();
        assert!(!apply_frost_stacks(&mut status, 2, 1.2));
        assert_eq!(status.frost_stacks, 2);
        assert!(!status.is_frozen());

        assert!(!apply_frost_stacks(&mut status, 2, 1.2));
        assert_eq!(status.frost_stacks, 4);

        assert!(apply_frost_stacks(&mut status, 1, 1.2));
        assert!(status.is_frozen());
        assert_eq!(status.frost_stacks, 0);
        assert!((status.freeze_timer - 1.2).abs() < 1e-5);
        assert!((status.freeze_immunity - 3.0).abs() < 1e-5);

        // Cannot re-trigger during freeze immunity
        assert!(!apply_frost_stacks(&mut status, 5, 1.2));
    }

    #[test]
    fn test_boss_slow_cap_and_frozen_speed() {
        let status = StatusEffects {
            slow_pct: 0.55,
            slow_timer: 3.0,
            freeze_timer: 1.2,
            ..default()
        };

        // Normal enemy is stunned (speed = 0.0) while frozen
        assert!((status.speed_multiplier(false, 0.65, 0.35) - 0.0).abs() < 1e-5);

        // Boss is stun-immune, gets +20% slow while frozen, clamped at max_slow_boss (0.35)
        let boss_mult = status.speed_multiplier(true, 0.65, 0.35);
        assert!((boss_mult - 0.65).abs() < 1e-5);
    }

    #[test]
    fn test_shatter_bonus_damage() {
        let normal_dmg = compute_wc3_damage(
            100.0,
            MovementDomain::Ground,
            0.0,
            false,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        );
        let shatter_80 = compute_wc3_damage(
            100.0,
            MovementDomain::Ground,
            0.0,
            false,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.80,
            1.0,
        );
        let shatter_110 = compute_wc3_damage(
            100.0,
            MovementDomain::Ground,
            0.0,
            false,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.10,
            1.0,
        );
        assert!((normal_dmg - 100.0).abs() < 1e-4);
        assert!((shatter_80 - 180.0).abs() < 1e-4);
        assert!((shatter_110 - 210.0).abs() < 1e-4);
    }
}
