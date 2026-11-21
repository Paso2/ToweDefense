use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy::prelude::*;
use pathfinding::prelude::astar;

use crate::types::{
    AIR_ALTITUDE, AppState, BuildBlockReason, Enemy, FlowField, FlowStage, GameConfigs,
    GameSpeed, GameSet, GridMap, MovementDomain, SelectionState, TileCoord, TileKind, WavePhase,
    WaveState,
};

pub struct PathfindingPlugin;

impl Plugin for PathfindingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                tick_reclaim_timers,
                update_flow_field_and_anti_juggle,
                update_hover_preview,
            )
                .chain()
                .in_set(GameSet::PathfindingSet)
                .run_if(in_state(AppState::Playing)),
        )
        .add_systems(
            Update,
            render_path_and_range_gizmos
                .in_set(GameSet::VfxUiSet)
                .run_if(|state: Res<State<AppState>>| {
                    matches!(state.get(), AppState::Playing | AppState::Paused)
                }),
        );
    }
}

#[cfg(test)]
const CARDINAL_DIRS: [(i32, i32, u16); 4] = [(1, 0, 10), (-1, 0, 10), (0, 1, 10), (0, -1, 10)];

const ALL_DIRS: [(i32, i32, u16); 8] = [
    (1, 0, 10),
    (-1, 0, 10),
    (0, 1, 10),
    (0, -1, 10),
    (1, 1, 14),
    (1, -1, 14),
    (-1, 1, 14),
    (-1, -1, 14),
];

#[inline]
fn is_cell_walkable(grid: &GridMap, coord: TileCoord, blocked_override: Option<TileCoord>) -> bool {
    if Some(coord) == blocked_override {
        return false;
    }
    grid.get(coord).is_some_and(TileKind::is_walkable)
}

#[inline]
fn can_step(
    grid: &GridMap,
    from: TileCoord,
    dx: i32,
    dz: i32,
    blocked_override: Option<TileCoord>,
) -> bool {
    let next = TileCoord::new(from.x + dx, from.z + dz);
    if !is_cell_walkable(grid, next, blocked_override) {
        return false;
    }
    if dx != 0 && dz != 0 {
        let side_x = TileCoord::new(from.x + dx, from.z);
        let side_z = TileCoord::new(from.x, from.z + dz);
        return is_cell_walkable(grid, side_x, blocked_override)
            && is_cell_walkable(grid, side_z, blocked_override);
    }
    true
}

fn build_stage_flow(
    grid: &GridMap,
    target_tiles: &[TileCoord],
    blocked_override: Option<TileCoord>,
) -> FlowStage {
    let total = (grid.width.max(0) * grid.height.max(0)) as usize;
    let mut dist = vec![u16::MAX; total];
    let mut dir = vec![Vec2::ZERO; total];
    if total == 0 || target_tiles.is_empty() {
        return FlowStage { dist, dir };
    }

    let mut heap: BinaryHeap<Reverse<(u16, i32, i32)>> = BinaryHeap::new();
    for &t in target_tiles {
        if is_cell_walkable(grid, t, blocked_override)
            && let Some(idx) = grid.idx(t)
        {
            dist[idx] = 0;
            heap.push(Reverse((0, t.x, t.z)));
        }
    }

    // Reverse Dijkstra from the stage target tiles across walkable cells.
    while let Some(Reverse((cost, cx, cz))) = heap.pop() {
        let cur = TileCoord::new(cx, cz);
        let Some(cur_idx) = grid.idx(cur) else {
            continue;
        };
        if cost > dist[cur_idx] {
            continue;
        }

        for &(dx, dz, step_cost) in &ALL_DIRS {
            let nx = cx + dx;
            let nz = cz + dz;
            let next = TileCoord::new(nx, nz);
            if !can_step(grid, cur, dx, dz, blocked_override) {
                continue;
            }
            let Some(next_idx) = grid.idx(next) else {
                continue;
            };
            let new_cost = cost.saturating_add(step_cost);
            if new_cost < dist[next_idx] {
                dist[next_idx] = new_cost;
                heap.push(Reverse((new_cost, nx, nz)));
            }
        }
    }

    // Compute smooth normalized flow direction pointing toward the steepest reachable descent.
    for z in 0..grid.height {
        for x in 0..grid.width {
            let cur = TileCoord::new(x, z);
            let Some(cur_idx) = grid.idx(cur) else {
                continue;
            };
            let cur_dist = dist[cur_idx];
            if cur_dist == 0 || cur_dist == u16::MAX {
                continue;
            }

            let mut best_neighbor: Option<(TileCoord, f32)> = None;
            let mut blended = Vec2::ZERO;

            for &(dx, dz, step_cost) in &ALL_DIRS {
                if !can_step(grid, cur, dx, dz, blocked_override) {
                    continue;
                }
                let neighbor = TileCoord::new(x + dx, z + dz);
                let Some(n_idx) = grid.idx(neighbor) else {
                    continue;
                };
                let n_dist = dist[n_idx];
                if n_dist >= cur_dist {
                    continue;
                }
                // Score by drop-per-step so diagonal vs cardinal ties stay geometrically consistent.
                let drop = (cur_dist - n_dist) as f32 / step_cost as f32;
                let step_vec = Vec2::new(dx as f32, dz as f32).normalize_or_zero();
                blended += step_vec * drop;

                let is_better = match best_neighbor {
                    None => true,
                    Some((_, best_drop)) => drop > best_drop,
                };
                if is_better {
                    best_neighbor = Some((neighbor, drop));
                }
            }

            let flow = if blended.length_squared() > 1e-4 {
                blended.normalize()
            } else if let Some((best, _)) = best_neighbor {
                Vec2::new((best.x - x) as f32, (best.z - z) as f32).normalize_or_zero()
            } else {
                Vec2::ZERO
            };
            dir[cur_idx] = flow;
        }
    }

    FlowStage { dist, dir }
}

fn rebuild_flow_field_with_override(
    grid: &GridMap,
    blocked_override: Option<TileCoord>,
) -> Vec<FlowStage> {
    let stage_count = grid.stage_count();
    let mut stages = Vec::with_capacity(stage_count);
    for stage_idx in 0..stage_count {
        let targets = grid.stage_target_tiles(stage_idx);
        stages.push(build_stage_flow(grid, targets, blocked_override));
    }
    stages
}

pub fn rebuild_flow_field(grid: &GridMap) -> Vec<FlowStage> {
    rebuild_flow_field_with_override(grid, None)
}

/// Shortest-path verification & segment tracer backed by `pathfinding::prelude::astar`,
/// enforcing the exact same no-corner-cutting rule and 10/14 weights as the flow field.
fn astar_stage_segment(
    grid: &GridMap,
    start: TileCoord,
    targets: &[TileCoord],
    blocked_override: Option<TileCoord>,
) -> Option<Vec<TileCoord>> {
    if targets.is_empty() || !is_cell_walkable(grid, start, blocked_override) {
        return None;
    }

    let result = astar(
        &start,
        |&pos| {
            let mut nexts = Vec::with_capacity(8);
            for &(dx, dz, cost) in &ALL_DIRS {
                if can_step(grid, pos, dx, dz, blocked_override) {
                    nexts.push((TileCoord::new(pos.x + dx, pos.z + dz), cost as u32));
                }
            }
            nexts
        },
        |pos| {
            targets
                .iter()
                .map(|t| {
                    let dx = pos.x.abs_diff(t.x);
                    let dz = pos.z.abs_diff(t.z);
                    let diag = dx.min(dz);
                    let straight = dx.max(dz) - diag;
                    diag * 14 + straight * 10
                })
                .min()
                .unwrap_or(0)
        },
        |pos| targets.contains(pos),
    );

    result.map(|(path, _)| path)
}

fn trace_ground_path_with_override(
    grid: &GridMap,
    stages: &[FlowStage],
    blocked_override: Option<TileCoord>,
) -> Vec<Vec3> {
    if stages.is_empty() || grid.spawns.is_empty() {
        return Vec::new();
    }

    // Pick the first spawn zone's tile with the shortest reachable route to Stage 0.
    let mut start_coord = None;
    let mut best_dist = u16::MAX;
    for spawn_zone in &grid.spawns {
        for &tile in spawn_zone {
            if let Some(idx) = grid.idx(tile)
                && stages[0].dist[idx] < best_dist
            {
                best_dist = stages[0].dist[idx];
                start_coord = Some(tile);
            }
        }
        if start_coord.is_some() {
            break;
        }
    }

    let Some(mut cur) = start_coord else {
        return Vec::new();
    };

    let mut waypoints = Vec::new();
    waypoints.push(grid.tile_to_world(cur));

    for (stage_idx, stage) in stages.iter().enumerate() {
        let targets = grid.stage_target_tiles(stage_idx);
        if targets.is_empty() {
            continue;
        }

        // Trace downstream along the flow-field integration gradient first; fallback to A* if needed.
        let max_steps = (grid.width.max(1) * grid.height.max(1)) as usize;
        let mut stage_path = Vec::new();
        let mut cursor = cur;
        let mut reached = targets.contains(&cursor);

        for _ in 0..max_steps {
            if reached {
                break;
            }
            let Some(cur_idx) = grid.idx(cursor) else {
                break;
            };
            let cur_dist = stage.dist[cur_idx];
            if cur_dist == u16::MAX {
                break;
            }

            let mut best_next: Option<(TileCoord, u16)> = None;
            for &(dx, dz, _) in &ALL_DIRS {
                if !can_step(grid, cursor, dx, dz, blocked_override) {
                    continue;
                }
                let cand = TileCoord::new(cursor.x + dx, cursor.z + dz);
                let Some(c_idx) = grid.idx(cand) else {
                    continue;
                };
                let c_dist = stage.dist[c_idx];
                if c_dist < cur_dist && best_next.is_none_or(|(_, bd)| c_dist < bd) {
                    best_next = Some((cand, c_dist));
                }
            }

            let Some((next, _)) = best_next else {
                break;
            };
            stage_path.push(next);
            cursor = next;
            if targets.contains(&cursor) {
                reached = true;
            }
        }

        if !reached
            && let Some(astar_nodes) = astar_stage_segment(grid, cur, targets, blocked_override)
        {
            stage_path = astar_nodes.into_iter().skip(1).collect();
            if let Some(&last) = stage_path.last() {
                cursor = last;
                reached = true;
            }
        }

        if !reached {
            return Vec::new();
        }

        for node in stage_path {
            waypoints.push(grid.tile_to_world(node));
        }
        cur = cursor;
    }

    waypoints
}

pub fn trace_ground_path(grid: &GridMap, stages: &[FlowStage]) -> Vec<Vec3> {
    trace_ground_path_with_override(grid, stages, None)
}

pub fn compute_air_path(grid: &GridMap) -> Vec<Vec3> {
    let mut points = Vec::new();
    if let Some(first_spawn) = grid.spawns.first()
        && !first_spawn.is_empty()
    {
        let mut p = grid.zone_center_world(first_spawn);
        p.y = AIR_ALTITUDE;
        points.push(p);
    } else {
        return points;
    }

    for cp in &grid.checkpoints {
        if !cp.is_empty() {
            let mut p = grid.zone_center_world(cp);
            p.y = AIR_ALTITUDE;
            points.push(p);
        }
    }

    if !grid.goals.is_empty() {
        let mut p = grid.zone_center_world(&grid.goals);
        p.y = AIR_ALTITUDE;
        points.push(p);
    }

    points
}

fn validate_placement_inner(
    grid: &GridMap,
    coord: TileCoord,
    living_ground_enemies: &[(Vec3, Option<usize>)],
    is_wave_active: bool,
) -> Result<Vec<Vec3>, BuildBlockReason> {
    if !grid.in_bounds(coord) {
        return Err(BuildBlockReason::OutOfBounds);
    }

    let Some(kind) = grid.get(coord) else {
        return Err(BuildBlockReason::OutOfBounds);
    };

    match kind {
        TileKind::Tower(_) => return Err(BuildBlockReason::OccupiedByTower),
        TileKind::Buildable => {}
        TileKind::BlockedTerrain | TileKind::Spawn(_) | TileKind::Checkpoint(_) | TileKind::Goal => {
            return Err(BuildBlockReason::NotBuildable);
        }
    }

    if is_wave_active && grid.reclaim_remaining(coord) > 0.0 {
        return Err(BuildBlockReason::ReclaimCooldown);
    }

    let tile_center = grid.tile_to_world(coord);
    for &(enemy_pos, _) in living_ground_enemies {
        let dx = enemy_pos.x - tile_center.x;
        let dz = enemy_pos.z - tile_center.z;
        if (dx * dx + dz * dz).sqrt() < 0.55 {
            return Err(BuildBlockReason::OccupiedByEnemy);
        }
    }

    let candidate_stages = rebuild_flow_field_with_override(grid, Some(coord));
    if candidate_stages.is_empty() {
        return Err(BuildBlockReason::BlocksPath);
    }

    // 1. Every spawn zone must reach stage 0.
    for spawn_zone in &grid.spawns {
        let spawn_connected = spawn_zone.iter().any(|&tile| {
            grid.idx(tile)
                .is_some_and(|idx| candidate_stages[0].dist[idx] != u16::MAX)
        });
        if !spawn_connected {
            return Err(BuildBlockReason::BlocksPath);
        }
    }

    // 2. Every checkpoint zone i must reach stage i + 1 (next checkpoint or Goal).
    for (cp_idx, cp_zone) in grid.checkpoints.iter().enumerate() {
        let next_stage_idx = cp_idx + 1;
        let Some(next_stage) = candidate_stages.get(next_stage_idx) else {
            return Err(BuildBlockReason::BlocksPath);
        };
        let cp_connected = cp_zone
            .iter()
            .any(|&tile| grid.idx(tile).is_some_and(|idx| next_stage.dist[idx] != u16::MAX));
        if !cp_connected {
            return Err(BuildBlockReason::BlocksPath);
        }
    }

    // 3. Every living ground enemy must still be able to reach its current stage target (or any stage if unspecified).
    for &(enemy_pos, enemy_stage) in living_ground_enemies {
        let Some(enemy_tile) = grid.world_to_tile(enemy_pos) else {
            continue;
        };
        if enemy_tile == coord {
            return Err(BuildBlockReason::OccupiedByEnemy);
        }
        let Some(tile_idx) = grid.idx(enemy_tile) else {
            return Err(BuildBlockReason::TrapsEnemy);
        };
        let reachable = match enemy_stage {
            Some(stage) => {
                let stage_idx = stage.min(candidate_stages.len() - 1);
                candidate_stages[stage_idx].dist[tile_idx] != u16::MAX
            }
            None => candidate_stages
                .iter()
                .any(|stage| stage.dist[tile_idx] != u16::MAX),
        };
        if !reachable {
            return Err(BuildBlockReason::TrapsEnemy);
        }
    }

    // Extra shortest-path verification via A* from first reachable spawn tile to Stage 0 target.
    if let Some(spawn_tile) = grid.spawns.first().and_then(|z| {
        z.iter()
            .copied()
            .find(|&t| grid.idx(t).is_some_and(|i| candidate_stages[0].dist[i] != u16::MAX))
    }) {
        let stage0_targets = grid.stage_target_tiles(0);
        if astar_stage_segment(grid, spawn_tile, stage0_targets, Some(coord)).is_none() {
            return Err(BuildBlockReason::BlocksPath);
        }
    }

    let preview_path = trace_ground_path_with_override(grid, &candidate_stages, Some(coord));
    if preview_path.is_empty() {
        return Err(BuildBlockReason::BlocksPath);
    }

    Ok(preview_path)
}

pub fn validate_placement_with_stages(
    grid: &GridMap,
    coord: TileCoord,
    living_ground_enemies: &[(Vec3, usize)],
    is_wave_active: bool,
) -> Result<Vec<Vec3>, BuildBlockReason> {
    let enemies: Vec<(Vec3, Option<usize>)> = living_ground_enemies
        .iter()
        .map(|&(pos, stage)| (pos, Some(stage)))
        .collect();
    validate_placement_inner(grid, coord, &enemies, is_wave_active)
}

pub fn validate_placement(
    grid: &GridMap,
    coord: TileCoord,
    living_ground_enemies: &[Vec3],
    is_wave_active: bool,
) -> Result<Vec<Vec3>, BuildBlockReason> {
    let enemies: Vec<(Vec3, Option<usize>)> =
        living_ground_enemies.iter().map(|&pos| (pos, None)).collect();
    validate_placement_inner(grid, coord, &enemies, is_wave_active)
}

fn tick_reclaim_timers(time: Res<Time>, speed: Res<GameSpeed>, mut grid: ResMut<GridMap>) {
    let dt = time.delta_secs().min(0.05) * speed.multiplier();
    if dt <= 0.0 {
        return;
    }
    for timer in &mut grid.reclaim_timers {
        if *timer > 0.0 {
            *timer = (*timer - dt).max(0.0);
        }
    }
}

fn update_flow_field_and_anti_juggle(
    mut grid: ResMut<GridMap>,
    mut flow: ResMut<FlowField>,
    wave: Res<WaveState>,
    configs: Res<GameConfigs>,
    mut enemies: Query<(&Transform, &mut Enemy, &mut crate::types::StatusEffects)>,
) {
    if !grid.dirty {
        return;
    }

    let stages = rebuild_flow_field(&grid);
    let base_ground_path = trace_ground_path(&grid, &stages);
    let air_path = compute_air_path(&grid);
    let base_path_len = base_ground_path.len().saturating_sub(1);

    flow.width = grid.width;
    flow.height = grid.height;
    flow.stages = stages;
    flow.base_ground_path = base_ground_path;
    flow.air_path = air_path;
    flow.base_path_len = base_path_len;
    grid.dirty = false;

    // Invalidate cached hover preview so it refreshes against the new grid topology.
    flow.hover_tile = None;

    let balance = &configs.balance;
    let is_wave_active = wave.phase == WavePhase::WaveActive;

    for (transform, mut enemy, mut status) in &mut enemies {
        if enemy.domain != MovementDomain::Ground || flow.stages.is_empty() {
            continue;
        }
        let stage_idx = enemy.current_stage.min(flow.stages.len() - 1);
        let Some(tile) = grid.world_to_tile(transform.translation) else {
            continue;
        };
        let Some(idx) = grid.idx(tile) else {
            continue;
        };
        let new_dist = flow.stages[stage_idx].dist[idx];
        if new_dist == u16::MAX {
            continue;
        }

        if is_wave_active && enemy.last_stage_dist != u16::MAX && new_dist > enemy.last_stage_dist {
            enemy.reroute_count = enemy.reroute_count.saturating_add(1);
            if enemy.reroute_count >= balance.anti_juggle_reroute_threshold
                && status.enrage_cooldown <= 0.0
            {
                status.enrage_timer = balance.anti_juggle_enrage_secs;
                status.enrage_cooldown = balance.anti_juggle_cooldown_secs;
                enemy.reroute_count = 0;
            }
        }
        enemy.last_stage_dist = new_dist;
    }
}

fn update_hover_preview(
    grid: Res<GridMap>,
    selection: Res<SelectionState>,
    wave: Res<WaveState>,
    mut flow: ResMut<FlowField>,
    enemies: Query<(&Transform, &Enemy)>,
) {
    if selection.build_tower_id.is_none() {
        if flow.hover_tile.is_some() {
            flow.hover_tile = None;
            flow.hover_valid = false;
            flow.hover_block_reason = None;
            flow.hover_ground_path.clear();
            flow.hover_path_len = 0;
        }
        return;
    }

    let Some(hovered) = selection.hovered_tile else {
        if flow.hover_tile.is_some() {
            flow.hover_tile = None;
            flow.hover_valid = false;
            flow.hover_block_reason = None;
            flow.hover_ground_path.clear();
            flow.hover_path_len = 0;
        }
        return;
    };

    if flow.hover_tile == Some(hovered) {
        return;
    }

    let living_ground: Vec<(Vec3, usize)> = enemies
        .iter()
        .filter(|(_, e)| e.domain == MovementDomain::Ground && e.hp > 0.0)
        .map(|(t, e)| (t.translation, e.current_stage))
        .collect();

    let is_wave_active = wave.phase == WavePhase::WaveActive;
    match validate_placement_with_stages(&grid, hovered, &living_ground, is_wave_active) {
        Ok(preview) => {
            flow.hover_tile = Some(hovered);
            flow.hover_valid = true;
            flow.hover_block_reason = None;
            flow.hover_path_len = preview.len().saturating_sub(1);
            flow.hover_ground_path = preview;
        }
        Err(reason) => {
            flow.hover_tile = Some(hovered);
            flow.hover_valid = false;
            flow.hover_block_reason = Some(reason);
            flow.hover_ground_path.clear();
            flow.hover_path_len = 0;
        }
    }
}

fn draw_tile_outline(gizmos: &mut Gizmos, center: Vec3, half: f32, y: f32, color: Color) {
    let a = Vec3::new(center.x - half, y, center.z - half);
    let b = Vec3::new(center.x + half, y, center.z - half);
    let c = Vec3::new(center.x + half, y, center.z + half);
    let d = Vec3::new(center.x - half, y, center.z + half);
    gizmos.line(a, b, color);
    gizmos.line(b, c, color);
    gizmos.line(c, d, color);
    gizmos.line(d, a, color);
}

#[allow(clippy::too_many_arguments)]
fn render_path_and_range_gizmos(
    mut gizmos: Gizmos,
    time: Res<Time>,
    grid: Res<GridMap>,
    flow: Res<FlowField>,
    wave: Res<WaveState>,
    selection: Res<SelectionState>,
    configs: Res<GameConfigs>,
    match_cfg: Res<crate::types::MatchConfig>,
    towers: Query<(&Transform, &crate::types::Tower)>,
) {
    let in_build_mode = selection.build_tower_id.is_some();
    let show_ground = flow.show_path_overlay || in_build_mode;
    let show_air = flow.show_path_overlay || wave.next_wave_has_air || in_build_mode;

    // 1. Ground path ribbon at y = 0.15 with dual outer glow rails, bright core, and bold animated chevrons.
    if show_ground {
        let is_blocked_hover = in_build_mode
            && flow.hover_tile.is_some()
            && (!flow.hover_valid || flow.hover_ground_path.is_empty());

        let path_pts = if in_build_mode
            && flow.hover_tile.is_some()
            && flow.hover_valid
            && !flow.hover_ground_path.is_empty()
        {
            &flow.hover_ground_path
        } else {
            &flow.base_ground_path
        };

        let (outer_color, core_color) = if is_blocked_hover {
            (
                Color::srgb(0.95, 0.18, 0.22),
                Color::srgb(1.0, 0.42, 0.36),
            )
        } else {
            (
                Color::srgb(0.30, 1.0, 0.58),
                Color::srgb(1.0, 0.92, 0.35),
            )
        };

        if path_pts.len() >= 2 {
            let y_core = 0.15;
            let y_rail = 0.14;
            let rail_offset = 0.055;
            let phase = (time.elapsed_secs() * 2.2).fract();
            for (idx, seg) in path_pts.windows(2).enumerate() {
                let p0_core = Vec3::new(seg[0].x, y_core, seg[0].z);
                let p1_core = Vec3::new(seg[1].x, y_core, seg[1].z);
                let dir = (p1_core - p0_core).normalize_or_zero();
                let right = Vec3::new(-dir.z, 0.0, dir.x);
                let side = right * rail_offset;

                // Dual outer glow rails
                let p0_rail = Vec3::new(seg[0].x, y_rail, seg[0].z);
                let p1_rail = Vec3::new(seg[1].x, y_rail, seg[1].z);
                gizmos.line(p0_rail + side, p1_rail + side, outer_color);
                gizmos.line(p0_rail - side, p1_rail - side, outer_color);

                // Bright center core line
                gizmos.line(p0_core, p1_core, core_color);

                if idx % 3 == 0 {
                    let t = (phase + (idx as f32 * 0.17)).fract();
                    let tip = p0_core.lerp(p1_core, t) + Vec3::Y * 0.03;
                    let back = tip - dir * 0.26;
                    gizmos.line(tip, back + right * 0.18, core_color);
                    gizmos.line(tip, back - right * 0.18, core_color);
                    let tip_outer = tip - dir * 0.06;
                    let back_outer = tip_outer - dir * 0.24;
                    gizmos.line(tip_outer, back_outer + right * 0.18, outer_color);
                    gizmos.line(tip_outer, back_outer - right * 0.18, outer_color);
                }
            }
        }
    }

    // 2. Dashed cyan air flight line at y = AIR_ALTITUDE (2.6) + vertical drop markers.
    if show_air && flow.air_path.len() >= 2 {
        let air_color = Color::srgba(0.24, 0.90, 1.0, 0.85);
        let drop_color = Color::srgba(0.24, 0.90, 1.0, 0.35);

        let mut air_segments: Vec<(Vec3, Vec3)> = flow
            .air_path
            .windows(2)
            .map(|seg| (seg[0], seg[1]))
            .collect();

        // For multi-spawn maps (e.g., WintermaulGlacier), also draw dashed air legs from extra spawns to Checkpoint 0.
        if grid.spawns.len() > 1 {
            let cp0_or_goal = flow.air_path[1];
            for spawn_zone in grid.spawns.iter().skip(1) {
                if !spawn_zone.is_empty() {
                    let mut p = grid.zone_center_world(spawn_zone);
                    p.y = AIR_ALTITUDE;
                    gizmos.line(
                        Vec3::new(p.x, 0.10, p.z),
                        Vec3::new(p.x, AIR_ALTITUDE, p.z),
                        drop_color,
                    );
                    gizmos.circle(
                        Isometry3d::new(
                            Vec3::new(p.x, 0.10, p.z),
                            Quat::from_rotation_arc(Vec3::Z, Vec3::Y),
                        ),
                        0.35,
                        drop_color,
                    );
                    air_segments.push((p, cp0_or_goal));
                }
            }
        }

        for point in &flow.air_path {
            gizmos.line(
                Vec3::new(point.x, 0.10, point.z),
                Vec3::new(point.x, AIR_ALTITUDE, point.z),
                drop_color,
            );
            gizmos.circle(
                Isometry3d::new(
                    Vec3::new(point.x, 0.10, point.z),
                    Quat::from_rotation_arc(Vec3::Z, Vec3::Y),
                ),
                0.35,
                drop_color,
            );
        }

        for (a, b) in air_segments {
            let total_len = a.distance(b);
            if total_len <= 0.01 {
                continue;
            }
            let dash_step = 0.9;
            let dash_len = 0.45;
            let count = (total_len / dash_step).ceil() as usize;
            let offset = (time.elapsed_secs() * 1.4).fract() * dash_step;
            for i in 0..=count {
                let d0 = (i as f32 * dash_step + offset - dash_step).clamp(0.0, total_len);
                let d1 = (d0 + dash_len).clamp(0.0, total_len);
                if d1 - d0 > 0.05 {
                    let p0 = a.lerp(b, d0 / total_len);
                    let p1 = a.lerp(b, d1 / total_len);
                    gizmos.line(p0, p1, air_color);
                }
            }
        }
    }

    // 3. Reclaim-cooldown tiles during active waves (subtle red X + outline).
    if wave.phase == WavePhase::WaveActive {
        let reclaim_color = Color::srgba(0.95, 0.20, 0.20, 0.70);
        for z in 0..grid.height {
            for x in 0..grid.width {
                let coord = TileCoord::new(x, z);
                if grid.reclaim_remaining(coord) > 0.0 {
                    let center = grid.tile_to_world(coord);
                    draw_tile_outline(&mut gizmos, center, 0.42, 0.09, reclaim_color);
                    gizmos.line(
                        Vec3::new(center.x - 0.34, 0.09, center.z - 0.34),
                        Vec3::new(center.x + 0.34, 0.09, center.z + 0.34),
                        reclaim_color,
                    );
                    gizmos.line(
                        Vec3::new(center.x + 0.34, 0.09, center.z - 0.34),
                        Vec3::new(center.x - 0.34, 0.09, center.z + 0.34),
                        reclaim_color,
                    );
                }
            }
        }
    }

    // 4. Build hover square + range preview circle.
    if let Some(build_id) = &selection.build_tower_id
        && let Some(hovered) = selection.hovered_tile
        && grid.in_bounds(hovered)
    {
        let center = grid.tile_to_world(hovered);
        let valid = flow.hover_tile == Some(hovered) && flow.hover_valid;
        let color = if valid {
            Color::srgb(0.20, 0.96, 0.42)
        } else {
            Color::srgb(0.96, 0.20, 0.20)
        };
        draw_tile_outline(&mut gizmos, center, 0.48, 0.10, color);
        draw_tile_outline(&mut gizmos, center, 0.44, 0.12, color);

        if let Some(tower_cfg) = configs.get_tower(match_cfg.faction_id, build_id)
            && tower_cfg.range > 0.0
        {
            gizmos
                .circle(
                    Isometry3d::new(
                        Vec3::new(center.x, 0.10, center.z),
                        Quat::from_rotation_arc(Vec3::Z, Vec3::Y),
                    ),
                    tower_cfg.range,
                    color.with_alpha(0.65),
                )
                .resolution(48);
        }
    }

    // 5. Range & selection rings for selected towers.
    for &entity in &selection.selected_towers {
        if let Ok((transform, tower)) = towers.get(entity) {
            let pos = Vec3::new(transform.translation.x, 0.10, transform.translation.z);
            let sel_color = Color::srgb(0.98, 0.85, 0.25);
            draw_tile_outline(&mut gizmos, pos, 0.48, 0.10, sel_color);
            if let Some(cfg) = configs.get_tower(tower.faction, &tower.id)
                && cfg.range > 0.0
            {
                gizmos
                    .circle(
                        Isometry3d::new(pos, Quat::from_rotation_arc(Vec3::Z, Vec3::Y)),
                        cfg.range,
                        sel_color.with_alpha(0.70),
                    )
                    .resolution(48);
            }
        }
    }
}

#[cfg(test)]
fn cardinal_neighbors_count(grid: &GridMap, coord: TileCoord) -> usize {
    CARDINAL_DIRS
        .iter()
        .filter(|&&(dx, dz, _)| can_step(grid, coord, dx, dz, None))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MapId, default_maps};

    fn grid_from_map(map_id: MapId) -> GridMap {
        let maps = default_maps();
        let cfg = maps.get(&map_id).expect("map config must exist");
        let total = (cfg.width * cfg.height) as usize;
        let mut grid = GridMap {
            map_id: cfg.id,
            biome: cfg.biome,
            width: cfg.width,
            height: cfg.height,
            tiles: vec![TileKind::Buildable; total],
            reclaim_timers: vec![0.0; total],
            spawns: cfg.spawns.iter().map(|z| z.tiles()).collect(),
            checkpoints: cfg.checkpoints.iter().map(|z| z.tiles()).collect(),
            goals: cfg.goal.tiles(),
            dirty: false,
            version: 0,
        };
        for &b in &cfg.blocked_tiles {
            grid.set(b, TileKind::BlockedTerrain);
        }
        for (idx, zone) in grid.spawns.clone().iter().enumerate() {
            for &c in zone {
                grid.set(c, TileKind::Spawn(idx as u8));
            }
        }
        for (idx, zone) in grid.checkpoints.clone().iter().enumerate() {
            for &c in zone {
                grid.set(c, TileKind::Checkpoint(idx as u8));
            }
        }
        for &c in &grid.goals.clone() {
            grid.set(c, TileKind::Goal);
        }
        grid.dirty = false;
        grid
    }

    #[test]
    fn multi_stage_flow_field_connectivity_both_maps() {
        for map_id in [MapId::LordaeronBastion, MapId::WintermaulGlacier] {
            let grid = grid_from_map(map_id);
            let stages = rebuild_flow_field(&grid);
            assert_eq!(stages.len(), grid.stage_count());

            for spawn in &grid.spawns {
                assert!(
                    spawn
                        .iter()
                        .all(|&c| stages[0].dist[grid.idx(c).unwrap()] != u16::MAX),
                    "spawn not connected on {map_id:?}"
                );
            }
            for (cp_idx, cp) in grid.checkpoints.iter().enumerate() {
                assert!(
                    cp.iter()
                        .all(|&c| stages[cp_idx + 1].dist[grid.idx(c).unwrap()] != u16::MAX),
                    "checkpoint {cp_idx} not connected on {map_id:?}"
                );
            }

            let path = trace_ground_path(&grid, &stages);
            assert!(
                path.len() > 10,
                "expected traced path on {map_id:?}, got {}",
                path.len()
            );
            for spawn in &grid.spawns {
                let s_idx = grid.idx(spawn[0]).unwrap();
                assert!(stages[0].dir[s_idx].length() > 0.5);
            }
            let air = compute_air_path(&grid);
            assert_eq!(air.len(), 2 + grid.checkpoints.len());
            assert!((air[0].y - AIR_ALTITUDE).abs() < 1e-4);
        }
    }

    #[test]
    fn no_corner_cutting_across_diagonal_tower_pair() {
        let mut grid = GridMap {
            map_id: MapId::LordaeronBastion,
            biome: crate::types::BiomeKind::LordaeronGrass,
            width: 5,
            height: 5,
            tiles: vec![TileKind::Buildable; 25],
            reclaim_timers: vec![0.0; 25],
            spawns: vec![vec![TileCoord::new(0, 2)]],
            checkpoints: vec![],
            goals: vec![TileCoord::new(4, 2)],
            dirty: false,
            version: 0,
        };
        grid.set(TileCoord::new(0, 2), TileKind::Spawn(0));
        grid.set(TileCoord::new(4, 2), TileKind::Goal);

        // Complete diagonal barrier (0,4),(1,3),(2,2),(3,1),(4,0) separating (0,0) from (4,4).
        let mut diag_grid = GridMap {
            map_id: MapId::LordaeronBastion,
            biome: crate::types::BiomeKind::LordaeronGrass,
            width: 5,
            height: 5,
            tiles: vec![TileKind::Buildable; 25],
            reclaim_timers: vec![0.0; 25],
            spawns: vec![vec![TileCoord::new(0, 0)]],
            checkpoints: vec![],
            goals: vec![TileCoord::new(4, 4)],
            dirty: false,
            version: 0,
        };
        diag_grid.set(TileCoord::new(0, 0), TileKind::Spawn(0));
        diag_grid.set(TileCoord::new(4, 4), TileKind::Goal);
        let dummy = Entity::from_raw_u32(1).unwrap();
        for i in 0..5 {
            diag_grid.set(TileCoord::new(i, 4 - i), TileKind::Tower(dummy));
        }

        let stages = rebuild_flow_field(&diag_grid);
        let spawn_idx = diag_grid.idx(TileCoord::new(0, 0)).unwrap();
        assert_eq!(
            stages[0].dist[spawn_idx],
            u16::MAX,
            "units must NOT squeeze diagonally across corner-touching towers"
        );
        assert_eq!(cardinal_neighbors_count(&grid, TileCoord::new(2, 2)), 4);
    }

    #[test]
    fn anti_block_rejects_wall_cutting_off_spawn_to_goal() {
        let mut grid = grid_from_map(MapId::LordaeronBastion);
        let dummy = Entity::from_raw_u32(2).unwrap();
        // Seal every column at x=12 except z=19, then try placing the last tower at (12, 19).
        for z in 0..(grid.height - 1) {
            let coord = TileCoord::new(12, z);
            if grid.get(coord) == Some(TileKind::Buildable) {
                grid.set(coord, TileKind::Tower(dummy));
            }
        }

        let res = validate_placement(&grid, TileCoord::new(12, grid.height - 1), &[], false);
        assert_eq!(res, Err(BuildBlockReason::BlocksPath));
    }

    #[test]
    fn anti_block_rejects_trapping_living_enemy_in_pocket() {
        let mut grid = grid_from_map(MapId::LordaeronBastion);
        let dummy = Entity::from_raw_u32(3).unwrap();
        // Build a 3-sided pocket around (10, 10) leaving only (11, 10) open.
        for (x, z) in [
            (9, 9),
            (10, 9),
            (11, 9),
            (9, 10),
            (9, 11),
            (10, 11),
            (11, 11),
        ] {
            grid.set(TileCoord::new(x, z), TileKind::Tower(dummy));
        }

        let enemy_pos = grid.tile_to_world(TileCoord::new(10, 10));
        assert_eq!(
            validate_placement(&grid, TileCoord::new(11, 10), &[enemy_pos], true),
            Err(BuildBlockReason::TrapsEnemy)
        );
        assert_eq!(
            validate_placement_with_stages(&grid, TileCoord::new(11, 10), &[(enemy_pos, 0)], true),
            Err(BuildBlockReason::TrapsEnemy)
        );
    }

    #[test]
    fn reclaim_cooldown_blocks_placement_during_active_wave() {
        let mut grid = grid_from_map(MapId::LordaeronBastion);
        let coord = TileCoord::new(8, 8);
        grid.set_reclaim_timer(coord, 1.5);

        assert_eq!(
            validate_placement(&grid, coord, &[], true),
            Err(BuildBlockReason::ReclaimCooldown)
        );
        // Outside active wave (BuildPrep), reclaim cooldown does not restrict building.
        assert!(validate_placement(&grid, coord, &[], false).is_ok());
    }

    #[test]
    fn anti_juggle_enrage_triggers_after_reroute_threshold() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let grid = grid_from_map(MapId::LordaeronBastion);
        let stages = rebuild_flow_field(&grid);
        let start_tile = TileCoord::new(5, 2);
        let start_dist = stages[0].dist[grid.idx(start_tile).unwrap()];

        app.insert_resource(grid);
        app.insert_resource(FlowField {
            width: 28,
            height: 20,
            stages,
            ..default()
        });
        app.insert_resource(WaveState {
            phase: WavePhase::WaveActive,
            ..default()
        });
        app.insert_resource(GameConfigs::default());
        app.add_systems(Update, update_flow_field_and_anti_juggle);

        let world_pos = app.world().resource::<GridMap>().tile_to_world(start_tile);
        let enemy_entity = app
            .world_mut()
            .spawn((
                Transform::from_translation(world_pos),
                Enemy {
                    archetype: crate::types::EnemyArchetypeId::GoblinRaider,
                    domain: MovementDomain::Ground,
                    hp: 45.0,
                    max_hp: 45.0,
                    base_speed: 2.1,
                    base_armor: 5.0,
                    slot_weight: 1.0,
                    bounty: 4,
                    bounty_paid: false,
                    is_boss: false,
                    is_elite: false,
                    current_stage: 0,
                    last_stage_dist: start_dist,
                    reroute_count: 2,
                    loop_count: 0,
                    regen_pct_per_sec: 0.0,
                    split_on_death: None,
                    ability: crate::types::BossAbilityKind::None,
                    ability_cooldown: 0.0,
                    shield_timer: 0.0,
                    anim_phase: 0.0,
                    visual_spawned: false,
                    scale: 1.0,
                },
                crate::types::StatusEffects::default(),
            ))
            .id();

        // Place a baffle between (5,2) and CP0 (24,2) so distance from (5,2) increases!
        let dummy = Entity::from_raw_u32(99).unwrap();
        {
            let mut grid = app.world_mut().resource_mut::<GridMap>();
            for z in 0..=8 {
                let c = TileCoord::new(8, z);
                if grid.get(c) == Some(TileKind::Buildable) {
                    grid.set(c, TileKind::Tower(dummy));
                }
            }
        }

        app.update();

        let status = app
            .world()
            .entity(enemy_entity)
            .get::<crate::types::StatusEffects>()
            .unwrap();
        let enemy = app.world().entity(enemy_entity).get::<Enemy>().unwrap();
        assert!(
            status.enrage_timer > 0.0,
            "expected anti-juggle enrage to trigger on 3rd reroute"
        );
        assert!(status.enrage_cooldown > 0.0);
        assert_eq!(enemy.reroute_count, 0);
    }
}
