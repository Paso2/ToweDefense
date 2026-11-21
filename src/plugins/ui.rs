#![allow(clippy::type_complexity, clippy::too_many_arguments)]

use bevy::{
    app::AppExit,
    picking::hover::Hovered,
    prelude::*,
    ui::Pressed,
    ui_widgets::{Activate, ActivateOnPress, Button as UiButton},
};

use crate::types::*;

// ============================================================================
// WC3 Fantasy Palette & Constants
// ============================================================================

const BG_PANEL: Color = Color::srgba(0.08, 0.09, 0.12, 0.88);
const BG_PANEL_DARK: Color = Color::srgba(0.05, 0.06, 0.09, 0.93);
const BG_OVERLAY: Color = Color::srgba(0.03, 0.04, 0.06, 0.78);
const BG_BTN_NORMAL: Color = Color::srgba(0.13, 0.14, 0.19, 0.95);
const BG_BTN_HOVER: Color = Color::srgba(0.22, 0.24, 0.31, 0.98);
const BG_BTN_PRESS: Color = Color::srgba(0.32, 0.27, 0.15, 0.98);
const BG_BTN_ACTIVE: Color = Color::srgba(0.24, 0.21, 0.12, 0.98);

const BORDER_GOLD: Color = Color::srgb(0.68, 0.56, 0.28);
const BORDER_GOLD_BRIGHT: Color = Color::srgb(0.95, 0.80, 0.38);
const BORDER_CRIMSON: Color = Color::srgb(0.92, 0.22, 0.22);

const TEXT_PRIMARY: Color = Color::srgb(0.94, 0.92, 0.86);
const TEXT_MUTED: Color = Color::srgb(0.70, 0.68, 0.62);
const TEXT_GOLD: Color = Color::srgb(0.96, 0.81, 0.34);
const TEXT_CYAN: Color = Color::srgb(0.42, 0.88, 1.0);
const TEXT_CRIMSON: Color = Color::srgb(1.0, 0.32, 0.32);

const MINIMAP_W: f32 = 160.0;
const MINIMAP_H: f32 = 104.0;
const MAX_MINIMAP_DOTS: usize = 96;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudDynamicState>()
            .init_resource::<SetupSelectionState>()
            // Screen setup / teardown
            .add_systems(OnEnter(AppState::MainMenu), spawn_main_menu)
            .add_systems(OnExit(AppState::MainMenu), despawn_marked::<MainMenuUi>)
            .add_systems(OnEnter(AppState::FractionSelect), spawn_fraction_select)
            .add_systems(
                OnExit(AppState::FractionSelect),
                despawn_marked::<FractionSelectUi>,
            )
            .add_systems(OnEnter(AppState::Playing), ensure_hud_spawned)
            .add_systems(OnEnter(AppState::MainMenu), despawn_marked::<HudRootUi>)
            .add_systems(OnEnter(AppState::FractionSelect), despawn_marked::<HudRootUi>)
            .add_systems(OnEnter(AppState::Paused), spawn_pause_menu)
            .add_systems(OnExit(AppState::Paused), despawn_marked::<PauseMenuUi>)
            .add_systems(OnEnter(AppState::Victory), spawn_victory_screen)
            .add_systems(OnExit(AppState::Victory), despawn_marked::<VictoryUi>)
            .add_systems(OnEnter(AppState::GameOver), spawn_game_over_screen)
            .add_systems(OnExit(AppState::GameOver), despawn_marked::<GameOverUi>)
            // Button activation observer (Bevy 0.20 ui_widgets)
            .add_observer(handle_ui_button_activate)
            // Update systems
            .add_systems(
                Update,
                (
                    update_button_visuals,
                    refresh_fraction_select_ui.run_if(in_state(AppState::FractionSelect)),
                    refresh_pause_menu_ui.run_if(in_state(AppState::Paused)),
                ),
            )
            .add_systems(
                Update,
                update_ui_pointer_capture
                    .in_set(GameSet::InputSet)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    handle_hud_hotkeys,
                    update_hud_top_bar,
                    update_boss_hp_bar,
                    update_status_banner,
                    update_minimap,
                    update_inspector_panel,
                    update_command_card,
                    update_command_tooltip,
                    spawn_and_animate_popups,
                )
                    .in_set(GameSet::VfxUiSet)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

// ============================================================================
// UI State & Marker Components
// ============================================================================

#[derive(Resource, Debug, Clone)]
struct SetupSelectionState {
    map_id: MapId,
    faction_id: FactionId,
    difficulty: Difficulty,
}

impl Default for SetupSelectionState {
    fn default() -> Self {
        Self {
            map_id: MapId::LordaeronBastion,
            faction_id: FactionId::ElementalCircle,
            difficulty: Difficulty::Normal,
        }
    }
}

#[derive(Resource, Debug, Default)]
struct HudDynamicState {
    last_command_signature: Option<(FactionId, Option<String>, Option<String>, usize, u32, u32)>,
    last_map_version: Option<(MapId, u64)>,
}

#[derive(Component)]
struct MainMenuUi;

#[derive(Component)]
struct FractionSelectUi;

#[derive(Component)]
struct HudRootUi;

#[derive(Component)]
struct PauseMenuUi;

#[derive(Component)]
struct VictoryUi;

#[derive(Component)]
struct GameOverUi;

#[derive(Component)]
struct UiHoverCapturePanel;

#[derive(Component, Clone, Debug)]
enum UiAction {
    // Main Menu
    GoToFractionSelect,
    QuickStartLordaeron,
    QuitGame,
    // Fraction Select
    SelectFaction(FactionId),
    SelectMap(MapId),
    SelectDifficulty(Difficulty),
    StartConfiguredMatch,
    BackToMainMenu,
    // In-Game Top Bar
    SendWaveEarly,
    SetGameSpeed(GameSpeed),
    ToggleMute,
    OpenPauseMenu,
    // Inspector
    CycleTargetPriority,
    SellSelectedTowers,
    // Command Card
    BuildTowerCmd(String),
    UpgradeTowerCmd(String),
    CancelBuildMode,
    // Pause Menu
    ResumeGame,
    ToggleEdgeScroll,
    TogglePopups,
    // Victory / Game Over
    ContinueEndless,
    RestartMatch,
}

#[derive(Component, Clone, Debug, Default)]
struct ButtonStyleMeta {
    tooltip: Option<String>,
    selected_highlight: bool,
    accent: Option<Color>,
}

// Top Bar refs
#[derive(Component)]
struct HudTopBarPanel;
#[derive(Component)]
struct HudGoldText;
#[derive(Component)]
struct HudSpecialText;
#[derive(Component)]
struct HudWaveText;
#[derive(Component)]
struct HudSlotsText;
#[derive(Component)]
struct HudSendWaveBtn;
#[derive(Component)]
struct HudSendWaveText;
#[derive(Component)]
struct HudSpeedBtn(GameSpeed);
#[derive(Component)]
struct HudMuteBtnText;

// Boss & Status Banner refs
#[derive(Component)]
struct HudBossPanel;
#[derive(Component)]
struct HudBossNameText;
#[derive(Component)]
struct HudBossFillBar;
#[derive(Component)]
struct HudBossHpText;

#[derive(Component)]
struct HudBannerPanel;
#[derive(Component)]
struct HudBannerText;

// Minimap refs
#[derive(Component)]
struct HudMinimapCanvas;
#[derive(Component)]
struct HudMinimapTilesLayer;
#[derive(Component)]
struct HudMinimapDot;
#[derive(Component)]
struct HudNextWaveText;

// Inspector refs
#[derive(Component)]
struct HudInspectorPanel;
#[derive(Component)]
struct HudInspectorTitleText;
#[derive(Component)]
struct HudInspectorStatsText;
#[derive(Component)]
struct HudInspectorSubText;
#[derive(Component)]
struct HudPriorityBtn;
#[derive(Component)]
struct HudPriorityBtnText;
#[derive(Component)]
struct HudSellBtn;
#[derive(Component)]
struct HudSellBtnText;

// Command Card refs
#[derive(Component)]
struct HudCommandGrid;
#[derive(Component)]
struct HudCommandHeaderText;
#[derive(Component)]
struct HudTooltipPanel;
#[derive(Component)]
struct HudTooltipText;

// Fraction Select dynamic refs
#[derive(Component)]
struct SetupFactionBtn(FactionId);
#[derive(Component)]
struct SetupMapBtn(MapId);
#[derive(Component)]
struct SetupDifficultyBtn(Difficulty);
#[derive(Component)]
struct SetupFactionDetailText;
#[derive(Component)]
struct SetupBestScoreText;

// Pause Menu dynamic refs
#[derive(Component)]
struct PauseMuteText;
#[derive(Component)]
struct PauseEdgeScrollText;
#[derive(Component)]
struct PausePopupsText;

// Floating Popups
#[derive(Component)]
struct HudPopupsLayer;
#[derive(Component)]
struct HudPopupNode {
    world_pos: Vec3,
    timer: f32,
    max_timer: f32,
    base_color: Color,
}

// ============================================================================
// Helper Constructors (Bevy 0.20 Native UI APIs)
// ============================================================================

fn ui_text(content: impl Into<String>, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (
        Text::new(content.into()),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

fn spawn_wc3_button(
    parent: &mut ChildSpawnerCommands,
    action: UiAction,
    label: impl Into<String>,
    font_size: f32,
    width: Val,
    height: Val,
    meta: ButtonStyleMeta,
) -> Entity {
    spawn_wc3_button_tagged(parent, action, label, font_size, width, height, meta, ())
}

#[allow(clippy::too_many_arguments)]
fn spawn_wc3_button_tagged<T: Bundle>(
    parent: &mut ChildSpawnerCommands,
    action: UiAction,
    label: impl Into<String>,
    font_size: f32,
    width: Val,
    height: Val,
    meta: ButtonStyleMeta,
    extra: T,
) -> Entity {
    let border_col = if meta.selected_highlight {
        BORDER_GOLD_BRIGHT
    } else {
        BORDER_GOLD
    };
    let bg_col = if meta.selected_highlight {
        BG_BTN_ACTIVE
    } else {
        BG_BTN_NORMAL
    };
    let text_col = meta.accent.unwrap_or(TEXT_PRIMARY);

    parent
        .spawn((
            UiButton,
            ActivateOnPress,
            Hovered::default(),
            action,
            meta,
            extra,
            Node {
                width,
                height,
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(4)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                padding: UiRect::axes(px(6), px(2)),
                ..default()
            },
            BorderColor::all(border_col),
            BackgroundColor(bg_col),
            children![(
                Text::new(label.into()),
                TextFont {
                    font_size: FontSize::Px(font_size),
                    ..default()
                },
                TextColor(text_col),
                TextLayout::new(Justify::Center, LineBreak::WordBoundary),
                Pickable::IGNORE,
            )],
        ))
        .id()
}

fn despawn_marked<M: Component>(mut commands: Commands, query: Query<Entity, With<M>>) {
    for entity in &query {
        if let Ok(mut ec) = commands.get_entity(entity) {
            ec.despawn();
        }
    }
}

// ============================================================================
// Main Menu
// ============================================================================

fn spawn_main_menu(mut commands: Commands, scores: Res<ProfileScores>) {
    let total_victories: u32 = scores.records.values().map(|r| r.victories).sum();
    let best_score: u64 = scores
        .records
        .values()
        .map(|r| r.best_score)
        .max()
        .unwrap_or(0);

    commands
        .spawn((
            MainMenuUi,
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BG_OVERLAY),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(520),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(px(28)),
                    row_gap: px(14),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(BG_PANEL_DARK),
                BorderColor::all(BORDER_GOLD),
            ))
            .with_children(|card| {
                card.spawn((
                    ui_text("AETHERMAUL TD", 38.0, TEXT_GOLD),
                    TextLayout::new(Justify::Center, LineBreak::NoWrap),
                ));
                card.spawn((
                    ui_text(
                        "3D Fantasy Mazing Tower Defense im Warcraft-3-Stil",
                        14.0,
                        TEXT_MUTED,
                    ),
                    TextLayout::new(Justify::Center, LineBreak::WordBoundary),
                ));

                card.spawn((
                    Node {
                        width: percent(100),
                        height: px(2),
                        margin: UiRect::vertical(px(4)),
                        ..default()
                    },
                    BackgroundColor(BORDER_GOLD.with_alpha(0.45)),
                ));

                spawn_wc3_button(
                    card,
                    UiAction::GoToFractionSelect,
                    "Spielen (Fraktion & Karte wählen)",
                    17.0,
                    percent(85),
                    px(46),
                    ButtonStyleMeta {
                        selected_highlight: true,
                        accent: Some(TEXT_GOLD),
                        ..default()
                    },
                );
                spawn_wc3_button(
                    card,
                    UiAction::QuickStartLordaeron,
                    "Schnellstart: Elementar-Zirkel auf Lordaeron",
                    15.0,
                    percent(85),
                    px(42),
                    ButtonStyleMeta::default(),
                );
                spawn_wc3_button(
                    card,
                    UiAction::QuitGame,
                    "Beenden",
                    15.0,
                    percent(85),
                    px(40),
                    ButtonStyleMeta {
                        accent: Some(TEXT_MUTED),
                        ..default()
                    },
                );

                let stats_str = if best_score > 0 || total_victories > 0 {
                    format!("Siege: {total_victories}  |  Bester Score: {best_score}")
                } else {
                    "5 Fraktionen • 2 Karten • 30 Wellen + Endless".to_string()
                };
                card.spawn((
                    ui_text(stats_str, 12.0, TEXT_MUTED),
                    Node {
                        margin: UiRect::top(px(6)),
                        ..default()
                    },
                ));
            });
        });
}

// ============================================================================
// Fraction / Map / Difficulty Select Screen
// ============================================================================

fn spawn_fraction_select(mut commands: Commands, setup: Res<SetupSelectionState>) {
    commands
        .spawn((
            FractionSelectUi,
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BG_OVERLAY),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(920),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(22)),
                    row_gap: px(14),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(BG_PANEL_DARK),
                BorderColor::all(BORDER_GOLD),
            ))
            .with_children(|panel| {
                panel.spawn(ui_text("SCHLACHT VORBEREITEN", 24.0, TEXT_GOLD));

                // Section 1: 5 Factions
                panel.spawn(ui_text("1. Fraktion wählen:", 14.0, TEXT_MUTED));
                panel
                    .spawn(Node {
                        width: percent(100),
                        flex_direction: FlexDirection::Row,
                        column_gap: px(8),
                        ..default()
                    })
                    .with_children(|row| {
                        for fid in FactionId::ALL {
                            let selected = setup.faction_id == fid;
                            spawn_wc3_button_tagged(
                                row,
                                UiAction::SelectFaction(fid),
                                format!("{}\n({})", fid.display_name(), fid.currency_name()),
                                13.0,
                                percent(20),
                                px(54),
                                ButtonStyleMeta {
                                    selected_highlight: selected,
                                    accent: Some(fid.accent_color()),
                                    ..default()
                                },
                                SetupFactionBtn(fid),
                            );
                        }
                    });

                panel.spawn((
                    SetupFactionDetailText,
                    ui_text(
                        format!(
                            "{}: {}",
                            setup.faction_id.display_name(),
                            setup.faction_id.short_tagline()
                        ),
                        14.0,
                        TEXT_PRIMARY,
                    ),
                ));

                // Section 2: Maps + Difficulty side-by-side
                panel
                    .spawn(Node {
                        width: percent(100),
                        flex_direction: FlexDirection::Row,
                        column_gap: px(16),
                        ..default()
                    })
                    .with_children(|cols| {
                        // Maps
                        cols.spawn(Node {
                            width: percent(52),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(6),
                            ..default()
                        })
                        .with_children(|col| {
                            col.spawn(ui_text("2. Karte wählen:", 14.0, TEXT_MUTED));
                            col.spawn(Node {
                                width: percent(100),
                                flex_direction: FlexDirection::Row,
                                column_gap: px(8),
                                ..default()
                            })
                            .with_children(|mrow| {
                                for mid in MapId::ALL {
                                    spawn_wc3_button_tagged(
                                        mrow,
                                        UiAction::SelectMap(mid),
                                        mid.display_name(),
                                        13.0,
                                        percent(50),
                                        px(42),
                                        ButtonStyleMeta {
                                            selected_highlight: setup.map_id == mid,
                                            ..default()
                                        },
                                        SetupMapBtn(mid),
                                    );
                                }
                            });
                        });

                        // Difficulty
                        cols.spawn(Node {
                            width: percent(48),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(6),
                            ..default()
                        })
                        .with_children(|col| {
                            col.spawn(ui_text("3. Schwierigkeit:", 14.0, TEXT_MUTED));
                            col.spawn(Node {
                                width: percent(100),
                                flex_direction: FlexDirection::Row,
                                column_gap: px(8),
                                ..default()
                            })
                            .with_children(|drow| {
                                for diff in Difficulty::ALL {
                                    spawn_wc3_button_tagged(
                                        drow,
                                        UiAction::SelectDifficulty(diff),
                                        diff.display_name(),
                                        14.0,
                                        percent(33),
                                        px(42),
                                        ButtonStyleMeta {
                                            selected_highlight: setup.difficulty == diff,
                                            ..default()
                                        },
                                        SetupDifficultyBtn(diff),
                                    );
                                }
                            });
                        });
                    });

                // Best score preview
                panel.spawn((
                    SetupBestScoreText,
                    ui_text("Rekord: Noch kein Eintrag", 13.0, TEXT_CYAN),
                ));

                // Action Row
                panel
                    .spawn(Node {
                        width: percent(100),
                        justify_content: JustifyContent::SpaceBetween,
                        margin: UiRect::top(px(8)),
                        ..default()
                    })
                    .with_children(|actions| {
                        spawn_wc3_button(
                            actions,
                            UiAction::BackToMainMenu,
                            "Zurück",
                            15.0,
                            px(160),
                            px(42),
                            ButtonStyleMeta::default(),
                        );
                        spawn_wc3_button(
                            actions,
                            UiAction::StartConfiguredMatch,
                            "Schlacht starten",
                            17.0,
                            px(240),
                            px(44),
                            ButtonStyleMeta {
                                selected_highlight: true,
                                accent: Some(TEXT_GOLD),
                                ..default()
                            },
                        );
                    });
            });
        });
}

fn refresh_fraction_select_ui(
    setup: Res<SetupSelectionState>,
    scores: Res<ProfileScores>,
    configs: Res<GameConfigs>,
    mut faction_btns: Query<(&SetupFactionBtn, &mut ButtonStyleMeta)>,
    mut map_btns: Query<(&SetupMapBtn, &mut ButtonStyleMeta), Without<SetupFactionBtn>>,
    mut diff_btns: Query<
        (&SetupDifficultyBtn, &mut ButtonStyleMeta),
        (Without<SetupFactionBtn>, Without<SetupMapBtn>),
    >,
    mut detail_text: Query<&mut Text, With<SetupFactionDetailText>>,
    mut score_text: Query<&mut Text, (With<SetupBestScoreText>, Without<SetupFactionDetailText>)>,
) {
    if !setup.is_changed() && !scores.is_changed() {
        return;
    }

    for (b, mut meta) in &mut faction_btns {
        meta.selected_highlight = b.0 == setup.faction_id;
    }
    for (b, mut meta) in &mut map_btns {
        meta.selected_highlight = b.0 == setup.map_id;
    }
    for (b, mut meta) in &mut diff_btns {
        meta.selected_highlight = b.0 == setup.difficulty;
    }

    if let Ok(mut txt) = detail_text.single_mut() {
        let starting_special = configs
            .get_faction(setup.faction_id)
            .map(|f| f.starting_currency)
            .unwrap_or(1);
        **txt = format!(
            "{} ({}, Start: {}): {}",
            setup.faction_id.display_name(),
            setup.faction_id.currency_name(),
            starting_special,
            setup.faction_id.short_tagline()
        );
    }

    if let Ok(mut txt) = score_text.single_mut() {
        let diff_cfg = configs.difficulty_cfg(setup.difficulty);
        let key = (setup.map_id, setup.faction_id, setup.difficulty);
        if let Some(rec) = scores.records.get(&key) {
            let clear_str = rec
                .fastest_clear_secs
                .map(|s| format!("{:.0}s", s))
                .unwrap_or_else(|| "—".to_string());
            **txt = format!(
                "Bestleistung ({} / {}): Welle {} | Score {} | Siege {} | Schnellster Clear: {}  |  Startgold: {}g, Limit: {} Slots",
                setup.map_id.display_name(),
                setup.difficulty.display_name(),
                rec.best_wave,
                rec.best_score,
                rec.victories,
                clear_str,
                diff_cfg.start_gold,
                diff_cfg.max_slots
            );
        } else {
            **txt = format!(
                "Noch kein Rekord für diese Kombination  |  Startgold: {}g | Gegner-Limit: {} Slots | HP: {:.2}x",
                diff_cfg.start_gold, diff_cfg.max_slots, diff_cfg.hp_mult
            );
        }
    }
}

// ============================================================================
// In-Game WC3 HUD (Strict >= 80% Free 3D Playfield)
// ============================================================================

fn ensure_hud_spawned(
    mut commands: Commands,
    existing: Query<Entity, With<HudRootUi>>,
    mut dyn_state: ResMut<HudDynamicState>,
) {
    if !existing.is_empty() {
        return;
    }
    dyn_state.last_command_signature = None;
    dyn_state.last_map_version = None;

    commands
        .spawn((
            HudRootUi,
            Node {
                width: percent(100),
                height: percent(100),
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            // Floating Popups Layer (behind HUD panels, non-blocking)
            root.spawn((
                HudPopupsLayer,
                Node {
                    width: percent(100),
                    height: percent(100),
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    ..default()
                },
                Pickable::IGNORE,
            ));

            // 1. TOP RESOURCE BAR (height: 38px <= 44px)
            root.spawn((
                HudTopBarPanel,
                UiHoverCapturePanel,
                Hovered::default(),
                Node {
                    width: percent(100),
                    height: px(38),
                    position_type: PositionType::Absolute,
                    top: px(0),
                    left: px(0),
                    padding: UiRect::axes(px(10), px(4)),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    border: UiRect::bottom(px(1.5)),
                    ..default()
                },
                BackgroundColor(BG_PANEL),
                BorderColor::all(BORDER_GOLD),
            ))
            .with_children(|bar| {
                // Left: Gold + Special Currency
                bar.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(16),
                    ..default()
                })
                .with_children(|left| {
                    left.spawn((HudGoldText, ui_text("Gold: 120g (+8g Zins)", 14.0, TEXT_GOLD)));
                    left.spawn((HudSpecialText, ui_text("Essenzen: 1", 14.0, TEXT_CYAN)));
                });

                // Center: Wave + Active Slots
                bar.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(18),
                    ..default()
                })
                .with_children(|center| {
                    center.spawn((HudWaveText, ui_text("Welle 0/30 | Prep 25s", 14.0, TEXT_PRIMARY)));
                    center.spawn((HudSlotsText, ui_text("Gegner: 0/45 Slots", 14.0, TEXT_PRIMARY)));
                });

                // Right: Early Send + Speed + Mute + Menu
                bar.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(5),
                    ..default()
                })
                .with_children(|right| {
                    right
                        .spawn((
                            HudSendWaveBtn,
                            UiButton,
                            ActivateOnPress,
                            Hovered::default(),
                            UiAction::SendWaveEarly,
                            ButtonStyleMeta {
                                selected_highlight: true,
                                accent: Some(TEXT_GOLD),
                                ..default()
                            },
                            Node {
                                height: px(28),
                                padding: UiRect::axes(px(8), px(2)),
                                border: UiRect::all(px(1.5)),
                                border_radius: BorderRadius::all(px(4)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(BG_BTN_ACTIVE),
                            BorderColor::all(BORDER_GOLD_BRIGHT),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                HudSendWaveText,
                                ui_text("[N] Senden (+0g)", 12.0, TEXT_GOLD),
                                Pickable::IGNORE,
                            ));
                        });

                    for spd in [
                        GameSpeed::Paused,
                        GameSpeed::Normal,
                        GameSpeed::Fast,
                        GameSpeed::Ultra,
                    ] {
                        spawn_wc3_button_tagged(
                            right,
                            UiAction::SetGameSpeed(spd),
                            spd.label(),
                            12.0,
                            px(30),
                            px(28),
                            ButtonStyleMeta {
                                selected_highlight: spd == GameSpeed::Normal,
                                ..default()
                            },
                            HudSpeedBtn(spd),
                        );
                    }

                    right
                        .spawn((
                            UiButton,
                            ActivateOnPress,
                            Hovered::default(),
                            UiAction::ToggleMute,
                            ButtonStyleMeta::default(),
                            Node {
                                width: px(34),
                                height: px(28),
                                border: UiRect::all(px(1.5)),
                                border_radius: BorderRadius::all(px(4)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(BG_BTN_NORMAL),
                            BorderColor::all(BORDER_GOLD),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                HudMuteBtnText,
                                ui_text("Ton", 11.0, TEXT_PRIMARY),
                                Pickable::IGNORE,
                            ));
                        });

                    spawn_wc3_button(
                        right,
                        UiAction::OpenPauseMenu,
                        "Menü",
                        11.0,
                        px(44),
                        px(28),
                        ButtonStyleMeta::default(),
                    );
                });
            });

            // 2. BOSS HP BAR (top-center, hidden unless active_boss.is_some())
            root.spawn((
                HudBossPanel,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(44),
                    left: percent(50),
                    margin: UiRect::left(px(-190)),
                    width: px(380),
                    height: px(36),
                    padding: UiRect::axes(px(10), px(4)),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(4)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(BG_PANEL_DARK),
                BorderColor::all(BORDER_CRIMSON),
                Pickable::IGNORE,
            ))
            .with_children(|boss| {
                boss.spawn(Node {
                    width: percent(100),
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((HudBossNameText, ui_text("BOSS", 12.0, TEXT_GOLD)));
                    row.spawn((HudBossHpText, ui_text("0 / 0", 11.0, TEXT_PRIMARY)));
                });
                boss.spawn((
                    Node {
                        width: percent(100),
                        height: px(8),
                        border_radius: BorderRadius::all(px(2)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.18, 0.05, 0.05, 0.9)),
                ))
                .with_children(|track| {
                    track.spawn((
                        HudBossFillBar,
                        Node {
                            width: percent(100),
                            height: percent(100),
                            border_radius: BorderRadius::all(px(2)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.88, 0.16, 0.18)),
                    ));
                });
            });

            // 3. STATUS BANNER (subtle toast below top bar)
            root.spawn((
                HudBannerPanel,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(44),
                    left: percent(50),
                    margin: UiRect::left(px(-180)),
                    width: px(360),
                    height: px(24),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(4)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(BG_PANEL_DARK),
                BorderColor::all(BORDER_GOLD),
                Pickable::IGNORE,
            ))
            .with_children(|b| {
                b.spawn((HudBannerText, ui_text("", 12.0, TEXT_GOLD)));
            });

            // 4. BOTTOM-LEFT COMPACT MINIMAP PANEL (175 x 138 px)
            root.spawn((
                UiHoverCapturePanel,
                Hovered::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(8),
                    bottom: px(8),
                    width: px(175),
                    height: px(138),
                    padding: UiRect::all(px(6)),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(5)),
                    ..default()
                },
                BackgroundColor(BG_PANEL),
                BorderColor::all(BORDER_GOLD),
            ))
            .with_children(|mm| {
                mm.spawn((
                    HudMinimapCanvas,
                    Node {
                        width: px(MINIMAP_W),
                        height: px(MINIMAP_H),
                        position_type: PositionType::Relative,
                        overflow: Overflow::clip(),
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.06, 0.08, 0.07)),
                    BorderColor::all(Color::srgba(0.55, 0.48, 0.28, 0.6)),
                ))
                .with_children(|canvas| {
                    canvas.spawn((
                        HudMinimapTilesLayer,
                        Node {
                            width: percent(100),
                            height: percent(100),
                            position_type: PositionType::Absolute,
                            left: px(0),
                            top: px(0),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ));
                    canvas
                        .spawn((
                            Node {
                                width: percent(100),
                                height: percent(100),
                                position_type: PositionType::Absolute,
                                left: px(0),
                                top: px(0),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ))
                        .with_children(|dots| {
                            for _ in 0..MAX_MINIMAP_DOTS {
                                dots.spawn((
                                    HudMinimapDot,
                                    Node {
                                        position_type: PositionType::Absolute,
                                        width: px(4),
                                        height: px(4),
                                        border_radius: BorderRadius::MAX,
                                        display: Display::None,
                                        ..default()
                                    },
                                    BackgroundColor(TEXT_CRIMSON),
                                    Pickable::IGNORE,
                                ));
                            }
                        });
                });

                mm.spawn((
                    HudNextWaveText,
                    ui_text("Nächste: W1 Goblin-Vorhut", 11.0, TEXT_PRIMARY),
                    TextLayout::new(Justify::Left, LineBreak::NoWrap),
                ));
            });

            // 5. BOTTOM-CENTER CONTEXTUAL INSPECTOR (340 x 108 px, Display::None when nothing selected)
            root.spawn((
                HudInspectorPanel,
                UiHoverCapturePanel,
                Hovered::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(50),
                    margin: UiRect::left(px(-170)),
                    bottom: px(8),
                    width: px(340),
                    height: px(108),
                    padding: UiRect::all(px(8)),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(5)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(BG_PANEL),
                BorderColor::all(BORDER_GOLD),
            ))
            .with_children(|insp| {
                insp.spawn((
                    HudInspectorTitleText,
                    ui_text("Turm-Inspektor", 14.0, TEXT_GOLD),
                ));
                insp.spawn((
                    HudInspectorStatsText,
                    ui_text("Schaden: 0 | Tempo: 1.0/s | Reichw: 4.0", 12.0, TEXT_PRIMARY),
                ));
                insp.spawn((
                    HudInspectorSubText,
                    ui_text("Kills: 0 | Ziel: Boden + Luft", 11.0, TEXT_MUTED),
                ));

                insp.spawn(Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: px(8),
                    ..default()
                })
                .with_children(|btns| {
                    btns.spawn((
                        HudPriorityBtn,
                        UiButton,
                        ActivateOnPress,
                        Hovered::default(),
                        UiAction::CycleTargetPriority,
                        ButtonStyleMeta::default(),
                        Node {
                            width: percent(52),
                            height: px(26),
                            border: UiRect::all(px(1.5)),
                            border_radius: BorderRadius::all(px(4)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(BG_BTN_NORMAL),
                        BorderColor::all(BORDER_GOLD),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            HudPriorityBtnText,
                            ui_text("[T] Ziel: Erster", 11.0, TEXT_PRIMARY),
                            Pickable::IGNORE,
                        ));
                    });

                    btns.spawn((
                        HudSellBtn,
                        UiButton,
                        ActivateOnPress,
                        Hovered::default(),
                        UiAction::SellSelectedTowers,
                        ButtonStyleMeta {
                            accent: Some(TEXT_CRIMSON),
                            ..default()
                        },
                        Node {
                            width: percent(46),
                            height: px(26),
                            border: UiRect::all(px(1.5)),
                            border_radius: BorderRadius::all(px(4)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(BG_BTN_NORMAL),
                        BorderColor::all(BORDER_GOLD),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            HudSellBtnText,
                            ui_text("[X] Verkaufen (+0g)", 11.0, TEXT_GOLD),
                            Pickable::IGNORE,
                        ));
                    });
                });
            });

            // 6. COMPACT HOVER TOOLTIP (above Command Card, Display::None unless hovered)
            root.spawn((
                HudTooltipPanel,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(8),
                    bottom: px(152),
                    width: px(260),
                    padding: UiRect::axes(px(8), px(5)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(4)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(BG_PANEL_DARK),
                BorderColor::all(BORDER_GOLD),
                Pickable::IGNORE,
            ))
            .with_children(|tt| {
                tt.spawn((
                    HudTooltipText,
                    ui_text("", 11.0, TEXT_PRIMARY),
                    TextLayout::new(Justify::Left, LineBreak::WordBoundary),
                ));
            });

            // 7. BOTTOM-RIGHT COMPACT COMMAND CARD (232 x 138 px)
            root.spawn((
                UiHoverCapturePanel,
                Hovered::default(),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(8),
                    bottom: px(8),
                    width: px(232),
                    height: px(138),
                    padding: UiRect::all(px(6)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(5)),
                    ..default()
                },
                BackgroundColor(BG_PANEL),
                BorderColor::all(BORDER_GOLD),
            ))
            .with_children(|cc| {
                cc.spawn((
                    HudCommandHeaderText,
                    ui_text("Bauen (Q / W / E)", 11.0, TEXT_GOLD),
                ));
                cc.spawn((
                    HudCommandGrid,
                    Node {
                        width: percent(100),
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: px(4),
                        row_gap: px(4),
                        align_content: AlignContent::FlexStart,
                        ..default()
                    },
                ));
            });
        });
}

// ============================================================================
// Pause, Victory & Game Over Overlays
// ============================================================================

fn spawn_pause_menu(mut commands: Commands, settings: Res<GameSettings>) {
    commands
        .spawn((
            PauseMenuUi,
            Node {
                width: percent(100),
                height: percent(100),
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BG_OVERLAY),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(360),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(px(22)),
                    row_gap: px(10),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(BG_PANEL_DARK),
                BorderColor::all(BORDER_GOLD),
            ))
            .with_children(|card| {
                card.spawn(ui_text("SPIEL PAUSIERT", 24.0, TEXT_GOLD));

                spawn_wc3_button(
                    card,
                    UiAction::ResumeGame,
                    "Fortsetzen",
                    16.0,
                    percent(92),
                    px(40),
                    ButtonStyleMeta {
                        selected_highlight: true,
                        accent: Some(TEXT_GOLD),
                        ..default()
                    },
                );

                let mute_lbl = if settings.muted {
                    "Sound: STUMM"
                } else {
                    "Sound: AN"
                };
                spawn_wc3_button_tagged(
                    card,
                    UiAction::ToggleMute,
                    mute_lbl,
                    14.0,
                    percent(92),
                    px(36),
                    ButtonStyleMeta::default(),
                    PauseMuteText,
                );

                let edge_lbl = if settings.edge_scrolling {
                    "Edge-Scroll: AN"
                } else {
                    "Edge-Scroll: AUS"
                };
                spawn_wc3_button_tagged(
                    card,
                    UiAction::ToggleEdgeScroll,
                    edge_lbl,
                    14.0,
                    percent(92),
                    px(36),
                    ButtonStyleMeta::default(),
                    PauseEdgeScrollText,
                );

                let pop_lbl = if settings.show_popups {
                    "Schadens-/Gold-Popups: AN"
                } else {
                    "Schadens-/Gold-Popups: AUS"
                };
                spawn_wc3_button_tagged(
                    card,
                    UiAction::TogglePopups,
                    pop_lbl,
                    14.0,
                    percent(92),
                    px(36),
                    ButtonStyleMeta::default(),
                    PausePopupsText,
                );

                spawn_wc3_button(
                    card,
                    UiAction::BackToMainMenu,
                    "Hauptmenü",
                    15.0,
                    percent(92),
                    px(38),
                    ButtonStyleMeta {
                        accent: Some(TEXT_CRIMSON),
                        ..default()
                    },
                );
            });
        });
}

fn refresh_pause_menu_ui(
    settings: Res<GameSettings>,
    mute_btns: Query<&Children, With<PauseMuteText>>,
    edge_btns: Query<&Children, With<PauseEdgeScrollText>>,
    pop_btns: Query<&Children, With<PausePopupsText>>,
    mut texts: Query<&mut Text>,
) {
    if !settings.is_changed() {
        return;
    }
    for children in &mute_btns {
        if let Some(&child) = children.first()
            && let Ok(mut t) = texts.get_mut(child)
        {
            **t = if settings.muted {
                "Sound: STUMM".to_string()
            } else {
                "Sound: AN".to_string()
            };
        }
    }
    for children in &edge_btns {
        if let Some(&child) = children.first()
            && let Ok(mut t) = texts.get_mut(child)
        {
            **t = if settings.edge_scrolling {
                "Edge-Scroll: AN".to_string()
            } else {
                "Edge-Scroll: AUS".to_string()
            };
        }
    }
    for children in &pop_btns {
        if let Some(&child) = children.first()
            && let Ok(mut t) = texts.get_mut(child)
        {
            **t = if settings.show_popups {
                "Schadens-/Gold-Popups: AN".to_string()
            } else {
                "Schadens-/Gold-Popups: AUS".to_string()
            };
        }
    }
}

fn spawn_victory_screen(
    mut commands: Commands,
    wave: Res<WaveState>,
    economy: Res<EconomyState>,
    match_cfg: Res<MatchConfig>,
) {
    commands
        .spawn((
            VictoryUi,
            Node {
                width: percent(100),
                height: percent(100),
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BG_OVERLAY),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(460),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(px(26)),
                    row_gap: px(12),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(BG_PANEL_DARK),
                BorderColor::all(BORDER_GOLD_BRIGHT),
            ))
            .with_children(|card| {
                card.spawn(ui_text("SIEG! AETHER-DREADLORD BESIEGT!", 24.0, TEXT_GOLD));
                card.spawn(ui_text(
                    format!(
                        "{} auf {} ({})",
                        match_cfg.faction_id.display_name(),
                        match_cfg.map_id.display_name(),
                        match_cfg.difficulty.display_name()
                    ),
                    14.0,
                    TEXT_MUTED,
                ));
                card.spawn(ui_text(
                    format!(
                        "Welle: {}  |  Kills: {}  |  Score: {}",
                        wave.current_wave, economy.total_kills, economy.score
                    ),
                    16.0,
                    TEXT_PRIMARY,
                ));
                card.spawn(ui_text(
                    format!("Zeit: {:.1}s", economy.match_elapsed_secs),
                    14.0,
                    TEXT_CYAN,
                ));

                spawn_wc3_button(
                    card,
                    UiAction::ContinueEndless,
                    "Endless fortsetzen (ab Welle 31)",
                    16.0,
                    percent(90),
                    px(44),
                    ButtonStyleMeta {
                        selected_highlight: true,
                        accent: Some(TEXT_GOLD),
                        ..default()
                    },
                );
                spawn_wc3_button(
                    card,
                    UiAction::BackToMainMenu,
                    "Hauptmenü",
                    15.0,
                    percent(90),
                    px(40),
                    ButtonStyleMeta::default(),
                );
            });
        });
}

fn spawn_game_over_screen(
    mut commands: Commands,
    wave: Res<WaveState>,
    economy: Res<EconomyState>,
    match_cfg: Res<MatchConfig>,
) {
    commands
        .spawn((
            GameOverUi,
            Node {
                width: percent(100),
                height: percent(100),
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BG_OVERLAY),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(450),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(px(26)),
                    row_gap: px(12),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(BG_PANEL_DARK),
                BorderColor::all(BORDER_CRIMSON),
            ))
            .with_children(|card| {
                card.spawn(ui_text("NIEDERLAGE — PORTAL ÜBERLAUFEN!", 22.0, TEXT_CRIMSON));
                card.spawn(ui_text(
                    format!(
                        "{} • {} ({})",
                        match_cfg.faction_id.display_name(),
                        match_cfg.map_id.display_name(),
                        match_cfg.difficulty.display_name()
                    ),
                    14.0,
                    TEXT_MUTED,
                ));
                card.spawn(ui_text(
                    format!(
                        "Welle erreicht: {}  |  Kills: {}  |  Leaks: {}",
                        wave.current_wave, economy.total_kills, wave.leaked_total
                    ),
                    15.0,
                    TEXT_PRIMARY,
                ));
                card.spawn(ui_text(
                    format!("Endscore: {}", economy.score),
                    16.0,
                    TEXT_GOLD,
                ));

                spawn_wc3_button(
                    card,
                    UiAction::RestartMatch,
                    "Neu starten",
                    16.0,
                    percent(88),
                    px(42),
                    ButtonStyleMeta {
                        selected_highlight: true,
                        accent: Some(TEXT_GOLD),
                        ..default()
                    },
                );
                spawn_wc3_button(
                    card,
                    UiAction::BackToMainMenu,
                    "Hauptmenü",
                    15.0,
                    percent(88),
                    px(40),
                    ButtonStyleMeta::default(),
                );
            });
        });
}

// ============================================================================
// Button Observer & Visuals (Bevy 0.20 ui_widgets)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn handle_ui_button_activate(
    trigger: On<Activate>,
    actions: Query<&UiAction>,
    mut next_state: ResMut<NextState<AppState>>,
    mut setup: ResMut<SetupSelectionState>,
    mut match_cfg: ResMut<MatchConfig>,
    mut game_speed: ResMut<GameSpeed>,
    mut settings: ResMut<GameSettings>,
    mut selection: ResMut<SelectionState>,
    mut start_writer: MessageWriter<StartMatchRequest>,
    mut send_wave_writer: MessageWriter<SendWaveEarlyRequest>,
    mut upgrade_writer: MessageWriter<UpgradeTowerRequest>,
    mut sell_writer: MessageWriter<SellTowerRequest>,
    mut save_writer: MessageWriter<SavePersistenceMsg>,
    mut sfx_writer: MessageWriter<PlaySfxMsg>,
    mut exit_writer: MessageWriter<AppExit>,
    mut towers: Query<&mut Tower>,
) {
    let Ok(action) = actions.get(trigger.entity) else {
        return;
    };

    sfx_writer.write(PlaySfxMsg {
        kind: SfxEventKind::UiClick,
        volume_scale: 0.7,
    });

    match action {
        UiAction::GoToFractionSelect => {
            next_state.set(AppState::FractionSelect);
        }
        UiAction::QuickStartLordaeron => {
            start_writer.write(StartMatchRequest {
                map_id: MapId::LordaeronBastion,
                faction_id: FactionId::ElementalCircle,
                difficulty: Difficulty::Normal,
            });
            next_state.set(AppState::Playing);
        }
        UiAction::QuitGame => {
            exit_writer.write(AppExit::Success);
        }
        UiAction::SelectFaction(fid) => {
            setup.faction_id = *fid;
        }
        UiAction::SelectMap(mid) => {
            setup.map_id = *mid;
        }
        UiAction::SelectDifficulty(diff) => {
            setup.difficulty = *diff;
        }
        UiAction::StartConfiguredMatch => {
            start_writer.write(StartMatchRequest {
                map_id: setup.map_id,
                faction_id: setup.faction_id,
                difficulty: setup.difficulty,
            });
            next_state.set(AppState::Playing);
        }
        UiAction::BackToMainMenu => {
            next_state.set(AppState::MainMenu);
        }
        UiAction::SendWaveEarly => {
            send_wave_writer.write(SendWaveEarlyRequest);
        }
        UiAction::SetGameSpeed(spd) => {
            *game_speed = *spd;
        }
        UiAction::ToggleMute => {
            settings.muted = !settings.muted;
            save_writer.write(SavePersistenceMsg);
        }
        UiAction::OpenPauseMenu => {
            next_state.set(AppState::Paused);
        }
        UiAction::CycleTargetPriority => {
            let mut next_prio = None;
            for &e in &selection.selected_towers {
                if let Ok(mut t) = towers.get_mut(e) {
                    let p = next_prio.unwrap_or_else(|| t.priority.next());
                    next_prio = Some(p);
                    t.priority = p;
                }
            }
        }
        UiAction::SellSelectedTowers => {
            for &e in &selection.selected_towers {
                sell_writer.write(SellTowerRequest { tower_entity: e });
            }
        }
        UiAction::BuildTowerCmd(tower_id) => {
            if selection.build_tower_id.as_deref() == Some(tower_id.as_str()) {
                selection.build_tower_id = None;
            } else {
                selection.build_tower_id = Some(tower_id.clone());
                selection.selected_towers.clear();
                selection.selected_enemy = None;
            }
        }
        UiAction::UpgradeTowerCmd(target_id) => {
            for &e in &selection.selected_towers {
                upgrade_writer.write(UpgradeTowerRequest {
                    tower_entity: e,
                    target_tower_id: target_id.clone(),
                    free: false,
                });
            }
        }
        UiAction::CancelBuildMode => {
            selection.build_tower_id = None;
        }
        UiAction::ResumeGame => {
            next_state.set(AppState::Playing);
        }
        UiAction::ToggleEdgeScroll => {
            settings.edge_scrolling = !settings.edge_scrolling;
            save_writer.write(SavePersistenceMsg);
        }
        UiAction::TogglePopups => {
            settings.show_popups = !settings.show_popups;
            save_writer.write(SavePersistenceMsg);
        }
        UiAction::ContinueEndless => {
            match_cfg.endless_active = true;
            next_state.set(AppState::Playing);
        }
        UiAction::RestartMatch => {
            start_writer.write(StartMatchRequest {
                map_id: match_cfg.map_id,
                faction_id: match_cfg.faction_id,
                difficulty: match_cfg.difficulty,
            });
            next_state.set(AppState::Playing);
        }
    }
}

fn update_button_visuals(
    mut buttons: Query<
        (
            &Hovered,
            Has<Pressed>,
            &ButtonStyleMeta,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<UiButton>,
    >,
) {
    for (hovered, pressed, meta, mut bg, mut border) in &mut buttons {
        if pressed {
            *bg = BG_BTN_PRESS.into();
            border.set_all(BORDER_GOLD_BRIGHT);
        } else if hovered.get() {
            *bg = BG_BTN_HOVER.into();
            border.set_all(BORDER_GOLD_BRIGHT);
        } else if meta.selected_highlight {
            *bg = BG_BTN_ACTIVE.into();
            border.set_all(BORDER_GOLD_BRIGHT);
        } else {
            *bg = BG_BTN_NORMAL.into();
            border.set_all(BORDER_GOLD);
        }
    }
}

// ============================================================================
// Pointer Capture & Hotkeys
// ============================================================================

fn update_ui_pointer_capture(
    mut selection: ResMut<SelectionState>,
    panels: Query<(&Hovered, &Node), With<UiHoverCapturePanel>>,
    buttons: Query<&Hovered, With<UiButton>>,
) {
    let over_panel = panels
        .iter()
        .any(|(hovered, node)| node.display != Display::None && hovered.get());
    let over_button = buttons.iter().any(|hovered| hovered.get());
    selection.ui_pointer_captured = over_panel || over_button;
}

#[allow(clippy::too_many_arguments)]
fn handle_hud_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    configs: Res<GameConfigs>,
    match_cfg: Res<MatchConfig>,
    mut selection: ResMut<SelectionState>,
    mut game_speed: ResMut<GameSpeed>,
    mut next_state: ResMut<NextState<AppState>>,
    mut send_wave_writer: MessageWriter<SendWaveEarlyRequest>,
    mut upgrade_writer: MessageWriter<UpgradeTowerRequest>,
    mut sell_writer: MessageWriter<SellTowerRequest>,
    mut towers: Query<&mut Tower>,
) {
    // Pause toggle with Escape (when nothing else to cancel)
    if keys.just_pressed(KeyCode::Escape) {
        if selection.build_tower_id.is_some() {
            selection.build_tower_id = None;
        } else if !selection.selected_towers.is_empty() || selection.selected_enemy.is_some() {
            selection.selected_towers.clear();
            selection.selected_enemy = None;
        } else {
            next_state.set(AppState::Paused);
        }
        return;
    }

    if keys.just_pressed(KeyCode::Space) {
        *game_speed = if *game_speed == GameSpeed::Paused {
            GameSpeed::Normal
        } else {
            GameSpeed::Paused
        };
    }
    if keys.just_pressed(KeyCode::Digit1) {
        *game_speed = GameSpeed::Normal;
    }
    if keys.just_pressed(KeyCode::Digit2) {
        *game_speed = GameSpeed::Fast;
    }
    if keys.just_pressed(KeyCode::Digit3) {
        *game_speed = GameSpeed::Ultra;
    }

    if keys.just_pressed(KeyCode::KeyN) {
        send_wave_writer.write(SendWaveEarlyRequest);
    }

    if keys.just_pressed(KeyCode::KeyT) && !selection.selected_towers.is_empty() {
        let mut next_prio = None;
        for &e in &selection.selected_towers {
            if let Ok(mut t) = towers.get_mut(e) {
                let p = next_prio.unwrap_or_else(|| t.priority.next());
                next_prio = Some(p);
                t.priority = p;
            }
        }
    }

    if keys.just_pressed(KeyCode::KeyX) && !selection.selected_towers.is_empty() {
        for &e in &selection.selected_towers {
            sell_writer.write(SellTowerRequest { tower_entity: e });
        }
        return;
    }

    // Command Card hotkeys Q, W, E, A, S, D
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

    let Some(hk) = pressed_hotkey else {
        return;
    };

    let Some(faction_cfg) = configs.get_faction(match_cfg.faction_id) else {
        return;
    };

    if let Some(&first_sel) = selection.selected_towers.first() {
        if let Ok(tower) = towers.get(first_sel)
            && let Some(tcfg) = faction_cfg.towers.get(&tower.id)
        {
            for upg_id in &tcfg.upgrades_to {
                if let Some(ucfg) = faction_cfg.towers.get(upg_id)
                    && ucfg.hotkey.eq_ignore_ascii_case(hk)
                {
                    for &e in &selection.selected_towers {
                        upgrade_writer.write(UpgradeTowerRequest {
                            tower_entity: e,
                            target_tower_id: upg_id.clone(),
                            free: false,
                        });
                    }
                    break;
                }
            }
        }
    } else {
        for root_id in &faction_cfg.root_buildable_towers {
            if let Some(rcfg) = faction_cfg.towers.get(root_id)
                && rcfg.hotkey.eq_ignore_ascii_case(hk)
            {
                if selection.build_tower_id.as_deref() == Some(root_id.as_str()) {
                    selection.build_tower_id = None;
                } else {
                    selection.build_tower_id = Some(root_id.clone());
                }
                break;
            }
        }
    }
}

// ============================================================================
// HUD Live Updates
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn update_hud_top_bar(
    time: Res<Time>,
    configs: Res<GameConfigs>,
    match_cfg: Res<MatchConfig>,
    economy: Res<EconomyState>,
    wave: Res<WaveState>,
    game_speed: Res<GameSpeed>,
    settings: Res<GameSettings>,
    mut top_bar: Query<&mut BorderColor, With<HudTopBarPanel>>,
    mut gold_q: Query<&mut Text, With<HudGoldText>>,
    mut special_q: Query<(&mut Text, &mut TextColor), (With<HudSpecialText>, Without<HudGoldText>)>,
    mut wave_q: Query<
        &mut Text,
        (
            With<HudWaveText>,
            Without<HudGoldText>,
            Without<HudSpecialText>,
        ),
    >,
    mut slots_q: Query<
        (&mut Text, &mut TextColor),
        (
            With<HudSlotsText>,
            Without<HudGoldText>,
            Without<HudSpecialText>,
            Without<HudWaveText>,
        ),
    >,
    mut send_btn_q: Query<&mut Node, With<HudSendWaveBtn>>,
    mut send_txt_q: Query<
        &mut Text,
        (
            With<HudSendWaveText>,
            Without<HudGoldText>,
            Without<HudSpecialText>,
            Without<HudWaveText>,
            Without<HudSlotsText>,
        ),
    >,
    mut speed_btns: Query<(&HudSpeedBtn, &mut ButtonStyleMeta)>,
    mut mute_txt_q: Query<
        &mut Text,
        (
            With<HudMuteBtnText>,
            Without<HudGoldText>,
            Without<HudSpecialText>,
            Without<HudWaveText>,
            Without<HudSlotsText>,
            Without<HudSendWaveText>,
        ),
    >,
) {
    let rate = configs.balance.interest_rate + economy.interest_rate_bonus;
    let cap = configs.balance.interest_cap + economy.interest_cap_bonus;
    let next_interest = ((economy.gold as f32 * rate).floor() as u32).min(cap);

    if let Ok(mut t) = gold_q.single_mut() {
        **t = format!("Gold: {}g (+{}g Zins)", economy.gold, next_interest);
    }

    if let Ok((mut t, mut col)) = special_q.single_mut() {
        **t = format!(
            "{}: {}",
            match_cfg.faction_id.currency_name(),
            economy.special_currency
        );
        col.0 = match_cfg.faction_id.accent_color();
    }

    if let Ok(mut t) = wave_q.single_mut() {
        let wave_label = if match_cfg.endless_active || wave.current_wave > wave.max_standard_waves
        {
            format!("Endless {}", wave.current_wave)
        } else {
            format!("Welle {}/{}", wave.current_wave, wave.max_standard_waves)
        };
        **t = match wave.phase {
            WavePhase::BuildPrep => format!("{wave_label} | Prep {:.0}s", wave.prep_timer.max(0.0)),
            WavePhase::WaveActive => format!("{wave_label} | Aktiv"),
        };
    }

    let used_slots = wave.active_slots.ceil() as u32;
    if let Ok((mut t, mut col)) = slots_q.single_mut() {
        if wave.grace_active {
            let pulse = (time.elapsed_secs() * 7.0).sin() * 0.5 + 0.5;
            col.0 = Color::srgb(1.0, 0.2 + 0.3 * pulse, 0.2 + 0.2 * pulse);
            **t = format!(
                "Gegner: {}/{}  ALARM: {:.1}s!",
                used_slots,
                wave.max_slots,
                wave.grace_timer.max(0.0)
            );
            if let Ok(mut border) = top_bar.single_mut() {
                border.set_all(BORDER_CRIMSON);
            }
        } else {
            col.0 = if used_slots * 4 >= wave.max_slots * 3 {
                TEXT_GOLD
            } else {
                TEXT_PRIMARY
            };
            **t = format!("Gegner: {}/{} Slots", used_slots, wave.max_slots);
            if let Ok(mut border) = top_bar.single_mut() {
                border.set_all(BORDER_GOLD);
            }
        }
    }

    if let Ok(mut btn_node) = send_btn_q.single_mut() {
        btn_node.display = if wave.phase == WavePhase::BuildPrep {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut t) = send_txt_q.single_mut() {
        let next_w = (wave.current_wave + 1) as f32;
        let bonus = (wave.prep_timer.max(0.0)
            * (configs.balance.early_send_base_per_sec
                + configs.balance.early_send_wave_scale * next_w))
            .floor() as u32;
        **t = format!("[N] Senden (+{bonus}g)");
    }

    for (spd_btn, mut meta) in &mut speed_btns {
        meta.selected_highlight = spd_btn.0 == *game_speed;
    }

    if let Ok(mut t) = mute_txt_q.single_mut() {
        **t = if settings.muted {
            "Aus".to_string()
        } else {
            "Ton".to_string()
        };
    }
}

fn update_boss_hp_bar(
    wave: Res<WaveState>,
    enemies: Query<&Enemy>,
    mut panel_q: Query<&mut Node, With<HudBossPanel>>,
    mut name_q: Query<&mut Text, With<HudBossNameText>>,
    mut hp_q: Query<&mut Text, (With<HudBossHpText>, Without<HudBossNameText>)>,
    mut fill_q: Query<&mut Node, (With<HudBossFillBar>, Without<HudBossPanel>)>,
) {
    let Ok(mut panel) = panel_q.single_mut() else {
        return;
    };
    let Some(boss_entity) = wave.active_boss else {
        panel.display = Display::None;
        return;
    };
    let Ok(boss) = enemies.get(boss_entity) else {
        panel.display = Display::None;
        return;
    };

    panel.display = Display::Flex;
    let frac = (boss.hp / boss.max_hp.max(1.0)).clamp(0.0, 1.0);
    if let Ok(mut n) = name_q.single_mut() {
        **n = boss.archetype.display_name().to_string();
    }
    if let Ok(mut h) = hp_q.single_mut() {
        **h = format!("{:.0} / {:.0}", boss.hp.max(0.0), boss.max_hp);
    }
    if let Ok(mut fill) = fill_q.single_mut() {
        fill.width = percent(frac * 100.0);
    }
}

fn update_status_banner(
    economy: Res<EconomyState>,
    mut panel_q: Query<&mut Node, With<HudBannerPanel>>,
    mut text_q: Query<&mut Text, With<HudBannerText>>,
) {
    let Ok(mut panel) = panel_q.single_mut() else {
        return;
    };
    if let Some((msg, timer)) = &economy.status_banner
        && *timer > 0.0
    {
        panel.display = Display::Flex;
        if let Ok(mut t) = text_q.single_mut() {
            **t = msg.clone();
        }
    } else {
        panel.display = Display::None;
    }
}

#[allow(clippy::too_many_arguments)]
fn update_minimap(
    mut commands: Commands,
    grid: Res<GridMap>,
    wave: Res<WaveState>,
    mut dyn_state: ResMut<HudDynamicState>,
    tiles_layer: Query<Entity, With<HudMinimapTilesLayer>>,
    children_q: Query<&Children>,
    enemies: Query<(&Transform, &Enemy)>,
    mut dots: Query<(&mut Node, &mut BackgroundColor), With<HudMinimapDot>>,
    mut next_wave_q: Query<&mut Text, With<HudNextWaveText>>,
) {
    if let Ok(mut t) = next_wave_q.single_mut() {
        let air_tag = if wave.next_wave_has_air {
            " [LUFT]"
        } else {
            ""
        };
        **t = format!("Nächste:{air_tag} {}", wave.next_wave_title);
    }

    if grid.width <= 0 || grid.height <= 0 {
        return;
    }

    // Rebuild static/maze tile cells only when GridMap version changes
    let current_ver = Some((grid.map_id, grid.version));
    if dyn_state.last_map_version != current_ver
        && let Ok(layer_entity) = tiles_layer.single()
    {
        dyn_state.last_map_version = current_ver;
        if let Ok(children) = children_q.get(layer_entity) {
            for child in children.iter() {
                if let Ok(mut ec) = commands.get_entity(child) {
                    ec.despawn();
                }
            }
        }

        let tile_w = MINIMAP_W / grid.width as f32;
        let tile_h = MINIMAP_H / grid.height as f32;

        commands.entity(layer_entity).with_children(|layer| {
            for z in 0..grid.height {
                for x in 0..grid.width {
                    let Some(kind) = grid.get(TileCoord::new(x, z)) else {
                        continue;
                    };
                    let color = match kind {
                        TileKind::Buildable => continue,
                        TileKind::BlockedTerrain => Color::srgb(0.24, 0.25, 0.28),
                        TileKind::Spawn(_) => Color::srgb(0.92, 0.30, 0.24),
                        TileKind::Checkpoint(_) => Color::srgb(0.32, 0.72, 0.98),
                        TileKind::Goal => Color::srgb(0.75, 0.35, 0.96),
                        TileKind::Tower(_) => Color::srgb(0.78, 0.66, 0.32),
                    };
                    layer.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(x as f32 * tile_w),
                            top: px(z as f32 * tile_h),
                            width: px(tile_w.ceil().max(1.0)),
                            height: px(tile_h.ceil().max(1.0)),
                            ..default()
                        },
                        BackgroundColor(color),
                        Pickable::IGNORE,
                    ));
                }
            }
        });
    }

    // Live enemy dots on minimap
    let half_w = grid.width as f32 * 0.5;
    let half_h = grid.height as f32 * 0.5;
    let mut enemy_iter = enemies.iter();

    for (mut node, mut bg) in &mut dots {
        if let Some((tf, enemy)) = enemy_iter.next() {
            let nx = ((tf.translation.x / TILE_SIZE + half_w) / grid.width as f32).clamp(0.0, 0.98);
            let nz =
                ((tf.translation.z / TILE_SIZE + half_h) / grid.height as f32).clamp(0.0, 0.98);
            node.display = Display::Flex;
            node.left = px(nx * MINIMAP_W - 2.0);
            node.top = px(nz * MINIMAP_H - 2.0);
            let sz = if enemy.is_boss { 6.0 } else { 4.0 };
            node.width = px(sz);
            node.height = px(sz);
            bg.0 = if enemy.is_boss {
                TEXT_GOLD
            } else if enemy.domain == MovementDomain::Air {
                TEXT_CYAN
            } else {
                TEXT_CRIMSON
            };
        } else {
            node.display = Display::None;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_inspector_panel(
    configs: Res<GameConfigs>,
    wave: Res<WaveState>,
    selection: Res<SelectionState>,
    towers: Query<&Tower>,
    enemies: Query<(&Enemy, &StatusEffects)>,
    mut panel_q: Query<&mut Node, With<HudInspectorPanel>>,
    mut title_q: Query<&mut Text, With<HudInspectorTitleText>>,
    mut stats_q: Query<&mut Text, (With<HudInspectorStatsText>, Without<HudInspectorTitleText>)>,
    mut sub_q: Query<
        &mut Text,
        (
            With<HudInspectorSubText>,
            Without<HudInspectorTitleText>,
            Without<HudInspectorStatsText>,
        ),
    >,
    mut prio_btn_q: Query<&mut Node, (With<HudPriorityBtn>, Without<HudInspectorPanel>)>,
    mut prio_txt_q: Query<
        &mut Text,
        (
            With<HudPriorityBtnText>,
            Without<HudInspectorTitleText>,
            Without<HudInspectorStatsText>,
            Without<HudInspectorSubText>,
        ),
    >,
    mut sell_btn_q: Query<
        &mut Node,
        (
            With<HudSellBtn>,
            Without<HudInspectorPanel>,
            Without<HudPriorityBtn>,
        ),
    >,
    mut sell_txt_q: Query<
        &mut Text,
        (
            With<HudSellBtnText>,
            Without<HudInspectorTitleText>,
            Without<HudInspectorStatsText>,
            Without<HudInspectorSubText>,
            Without<HudPriorityBtnText>,
        ),
    >,
) {
    let Ok(mut panel) = panel_q.single_mut() else {
        return;
    };

    // Tower selection takes priority
    let selected_towers: Vec<&Tower> = selection
        .selected_towers
        .iter()
        .filter_map(|&e| towers.get(e).ok())
        .collect();

    if !selected_towers.is_empty() {
        panel.display = Display::Flex;
        if let Ok(mut n) = prio_btn_q.single_mut() {
            n.display = Display::Flex;
        }
        if let Ok(mut n) = sell_btn_q.single_mut() {
            n.display = Display::Flex;
        }

        let first = selected_towers[0];
        let tcfg = configs.get_tower(first.faction, &first.id);
        let name = tcfg.map(|c| c.name.as_str()).unwrap_or(first.id.as_str());
        let total_kills: u32 = selected_towers.iter().map(|t| t.kills).sum();

        if let Ok(mut t) = title_q.single_mut() {
            if selected_towers.len() > 1 {
                **t = format!("{}x {} (T{})", selected_towers.len(), name, first.tier);
            } else {
                **t = format!("{name} (Tier {})", first.tier);
            }
        }
        if let Ok(mut t) = stats_q.single_mut() {
            if let Some(cfg) = tcfg {
                let dps = cfg.damage * cfg.attack_speed;
                **t = format!(
                    "Schaden: {:.0} | Tempo: {:.2}/s | DPS: {:.0} | Reichw: {:.1}",
                    cfg.damage, cfg.attack_speed, dps, cfg.range
                );
            } else {
                **t = format!("Kills: {total_kills}");
            }
        }
        if let Ok(mut t) = sub_q.single_mut() {
            let mask_lbl = tcfg.map(|c| c.target_mask.label()).unwrap_or("—");
            **t = format!(
                "Kills: {total_kills} | Ziel: {mask_lbl} | Gesamtschaden: {:.0}",
                selected_towers.iter().map(|tw| tw.total_damage).sum::<f32>()
            );
        }
        if let Ok(mut t) = prio_txt_q.single_mut() {
            **t = format!("[T] Ziel: {}", first.priority.label());
        }

        let total_refund: u32 = selected_towers
            .iter()
            .map(|tw| {
                let rate = if wave.phase == WavePhase::BuildPrep {
                    if tw.built_in_prep_wave == Some(wave.current_wave) {
                        configs.balance.sell_refund_same_prep
                    } else {
                        configs.balance.sell_refund_prep
                    }
                } else {
                    configs.balance.sell_refund_active_wave
                };
                (tw.invested_gold as f32 * rate).floor() as u32
            })
            .sum();

        if let Ok(mut t) = sell_txt_q.single_mut() {
            **t = format!("[X] Verkaufen (+{total_refund}g)");
        }
        return;
    }

    // Enemy selection
    if let Some(enemy_entity) = selection.selected_enemy
        && let Ok((enemy, status)) = enemies.get(enemy_entity)
    {
        panel.display = Display::Flex;
        if let Ok(mut n) = prio_btn_q.single_mut() {
            n.display = Display::None;
        }
        if let Ok(mut n) = sell_btn_q.single_mut() {
            n.display = Display::None;
        }

        let eff_armor = (enemy.base_armor - status.armor_shred).max(-45.0);
        if let Ok(mut t) = title_q.single_mut() {
            **t = enemy.archetype.display_name().to_string();
        }
        if let Ok(mut t) = stats_q.single_mut() {
            **t = format!(
                "HP: {:.0}/{:.0} | Rüstung: {:.0} (eff. {:.0}) | Loops: {}",
                enemy.hp.max(0.0),
                enemy.max_hp,
                enemy.base_armor,
                eff_armor,
                enemy.loop_count
            );
        }
        if let Ok(mut t) = sub_q.single_mut() {
            let mut tags = Vec::new();
            if status.is_frozen() {
                tags.push("Gefroren");
            } else if status.slow_timer > 0.0 {
                tags.push("Verlangsamt");
            }
            if status.is_burning() {
                tags.push("Brennt");
            }
            if status.is_poisoned() {
                tags.push("Vergiftet");
            }
            if status.curse_timer > 0.0 {
                tags.push("Verflucht");
            }
            if status.armor_shred_timer > 0.0 {
                tags.push("Rüstung zerrissen");
            }
            let status_str = if tags.is_empty() {
                "Keine aktiven Debuffs".to_string()
            } else {
                tags.join(", ")
            };
            **t = format!("Status: {status_str}");
        }
        return;
    }

    // Nothing selected -> hidden ("nicht überladen")
    panel.display = Display::None;
}

#[allow(clippy::too_many_arguments)]
fn update_command_card(
    mut commands: Commands,
    configs: Res<GameConfigs>,
    match_cfg: Res<MatchConfig>,
    economy: Res<EconomyState>,
    selection: Res<SelectionState>,
    towers: Query<&Tower>,
    mut dyn_state: ResMut<HudDynamicState>,
    grid_q: Query<Entity, With<HudCommandGrid>>,
    children_q: Query<&Children>,
    mut header_q: Query<&mut Text, With<HudCommandHeaderText>>,
) {
    let selected_tower = selection
        .selected_towers
        .first()
        .and_then(|&e| towers.get(e).ok());

    let sig = (
        match_cfg.faction_id,
        selected_tower.map(|t| t.id.clone()),
        selection.build_tower_id.clone(),
        selection.selected_towers.len(),
        economy.gold,
        economy.special_currency,
    );

    if dyn_state.last_command_signature.as_ref() == Some(&sig) {
        return;
    }
    dyn_state.last_command_signature = Some(sig);

    let Some(faction_cfg) = configs.get_faction(match_cfg.faction_id) else {
        return;
    };
    let Ok(grid_entity) = grid_q.single() else {
        return;
    };

    if let Ok(children) = children_q.get(grid_entity) {
        for child in children.iter() {
            if let Ok(mut ec) = commands.get_entity(child) {
                ec.despawn();
            }
        }
    }

    let currency_short = match match_cfg.faction_id {
        FactionId::ElementalCircle => "E",
        FactionId::Citadel => "K",
        FactionId::DragonClan => "G",
        FactionId::Frostguard => "F",
        FactionId::Necropolis => "S",
    };

    if let Some(tower) = selected_tower {
        if let Ok(mut hdr) = header_q.single_mut() {
            **hdr = "Upgrades (Q / W / E)".to_string();
        }
        let upgrades = faction_cfg
            .towers
            .get(&tower.id)
            .map(|c| c.upgrades_to.clone())
            .unwrap_or_default();

        commands.entity(grid_entity).with_children(|grid| {
            if upgrades.is_empty() {
                grid.spawn(ui_text("Maximale Stufe erreicht.", 12.0, TEXT_MUTED));
                return;
            }
            for upg_id in upgrades {
                let Some(ucfg) = faction_cfg.towers.get(&upg_id) else {
                    continue;
                };
                let affordable = economy.gold >= ucfg.gold_cost
                    && economy.special_currency >= ucfg.special_cost;
                let cost_str = if ucfg.special_cost > 0 {
                    format!("{}g + {}{}", ucfg.gold_cost, ucfg.special_cost, currency_short)
                } else {
                    format!("{}g", ucfg.gold_cost)
                };
                let is_dual = ucfg.description.contains("Dual");
                let badge = if is_dual { " [Dual]" } else { "" };
                let label = format!("[{}] {}{}\n{}", ucfg.hotkey, ucfg.name, badge, cost_str);
                let tooltip = format!(
                    "{} ({cost_str}) | {} | DPS {:.0}\n{}",
                    ucfg.name,
                    ucfg.target_mask.label(),
                    ucfg.damage * ucfg.attack_speed,
                    ucfg.description
                );
                spawn_wc3_button(
                    grid,
                    UiAction::UpgradeTowerCmd(upg_id),
                    label,
                    10.5,
                    px(106),
                    px(48),
                    ButtonStyleMeta {
                        tooltip: Some(tooltip),
                        selected_highlight: false,
                        accent: Some(if affordable {
                            if is_dual { TEXT_CYAN } else { TEXT_PRIMARY }
                        } else {
                            TEXT_CRIMSON
                        }),
                    },
                );
            }
        });
    } else {
        if let Ok(mut hdr) = header_q.single_mut() {
            **hdr = if selection.build_tower_id.is_some() {
                "Bau-Modus aktiv (Rechtsklick: Abbruch)".to_string()
            } else {
                "Bauen (Q / W / E)".to_string()
            };
        }

        commands.entity(grid_entity).with_children(|grid| {
            for root_id in &faction_cfg.root_buildable_towers {
                let Some(rcfg) = faction_cfg.towers.get(root_id) else {
                    continue;
                };
                let affordable = economy.gold >= rcfg.gold_cost
                    && economy.special_currency >= rcfg.special_cost;
                let cost_str = if rcfg.special_cost > 0 {
                    format!("{}g + {}{}", rcfg.gold_cost, rcfg.special_cost, currency_short)
                } else {
                    format!("{}g", rcfg.gold_cost)
                };
                let active = selection.build_tower_id.as_deref() == Some(root_id.as_str());
                let label = format!("[{}] {}\n{}", rcfg.hotkey, rcfg.name, cost_str);
                let tooltip = format!(
                    "{} ({cost_str}) | {}\n{}",
                    rcfg.name,
                    rcfg.target_mask.label(),
                    rcfg.description
                );
                spawn_wc3_button(
                    grid,
                    UiAction::BuildTowerCmd(root_id.clone()),
                    label,
                    10.5,
                    px(106),
                    px(48),
                    ButtonStyleMeta {
                        tooltip: Some(tooltip),
                        selected_highlight: active,
                        accent: Some(if affordable { TEXT_PRIMARY } else { TEXT_CRIMSON }),
                    },
                );
            }
            if selection.build_tower_id.is_some() {
                spawn_wc3_button(
                    grid,
                    UiAction::CancelBuildMode,
                    "[Esc] Abbruch",
                    10.5,
                    px(106),
                    px(32),
                    ButtonStyleMeta {
                        accent: Some(TEXT_MUTED),
                        ..default()
                    },
                );
            }
        });
    }
}

fn update_command_tooltip(
    hovered_btns: Query<(&Hovered, &ButtonStyleMeta), With<UiButton>>,
    mut tt_panel: Query<&mut Node, With<HudTooltipPanel>>,
    mut tt_text: Query<&mut Text, With<HudTooltipText>>,
) {
    let Ok(mut panel) = tt_panel.single_mut() else {
        return;
    };

    let active_tooltip = hovered_btns
        .iter()
        .find_map(|(h, meta)| if h.get() { meta.tooltip.as_deref() } else { None });

    if let Some(tip) = active_tooltip {
        panel.display = Display::Flex;
        if let Ok(mut t) = tt_text.single_mut() {
            **t = tip.to_string();
        }
    } else {
        panel.display = Display::None;
    }
}

// ============================================================================
// Throttled Floating Popups (`SpawnPopupMsg`)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn spawn_and_animate_popups(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<GameSettings>,
    mut popup_reader: MessageReader<SpawnPopupMsg>,
    layer_q: Query<Entity, With<HudPopupsLayer>>,
    camera_q: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut active_popups: Query<(Entity, &mut HudPopupNode, &mut Node, &mut TextColor)>,
) {
    let dt = time.delta_secs();
    let mut active_count = 0usize;

    for (entity, mut pop, mut node, mut color) in &mut active_popups {
        pop.timer -= dt;
        if pop.timer <= 0.0 {
            commands.entity(entity).insert(PendingDespawn);
            continue;
        }
        active_count += 1;
        pop.world_pos.y += 0.9 * dt;
        let alpha = (pop.timer / pop.max_timer).clamp(0.0, 1.0);
        color.0 = pop.base_color.with_alpha(alpha);

        if let Ok((cam, cam_tf)) = camera_q.single()
            && let Ok(vp) = cam.world_to_viewport(cam_tf, pop.world_pos)
        {
            node.left = px(vp.x - 16.0);
            node.top = px(vp.y - 10.0);
        }
    }

    let Ok(layer_entity) = layer_q.single() else {
        popup_reader.clear();
        return;
    };

    if !settings.show_popups {
        popup_reader.clear();
        return;
    }

    let Ok((cam, cam_tf)) = camera_q.single() else {
        popup_reader.clear();
        return;
    };

    for msg in popup_reader.read() {
        if active_count >= MAX_ACTIVE_POPUPS {
            break;
        }
        let Ok(vp) = cam.world_to_viewport(cam_tf, msg.pos) else {
            continue;
        };
        active_count += 1;
        commands.entity(layer_entity).with_children(|layer| {
            layer.spawn((
                HudPopupNode {
                    world_pos: msg.pos,
                    timer: 0.95,
                    max_timer: 0.95,
                    base_color: msg.color,
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: px(vp.x - 16.0),
                    top: px(vp.y - 10.0),
                    ..default()
                },
                Text::new(msg.text.clone()),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(msg.color),
                Pickable::IGNORE,
            ));
        });
    }
}
