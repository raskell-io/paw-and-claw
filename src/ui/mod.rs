use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};

use crate::game::{
    TurnState, TurnPhase, Unit, FactionMember, Faction, GridPosition, MovementHighlights,
    PendingAction, ProductionState, AttackEvent, CaptureEvent, JoinEvent, ResupplyEvent, LoadEvent,
    UnloadEvent, GameMap, Terrain, Tile, UnitType, estimate_damage, GameResult,
    FogOfWar, Commanders, CommanderId, MapId, MapData,
    Weather, screen_to_grid, GameData, CancelMoveEvent, GridCursor, BattleConfig,
};
use crate::states::GameState;

mod editor;
mod boot;
mod hud;
mod icons;
mod input;
mod menu;
mod map_menu;
mod overlays;
mod production;
mod setup;
mod theme;

/// Types of actions available in the action menu
#[derive(Debug, Clone)]
pub enum MenuAction {
    Attack(Entity),  // Entity is the target
    Capture,
    Join,
    Resupply,
    Load((i32, i32)),  // Position of the transport the unit moved onto
    Unload(usize, (i32, i32)), // Which passenger, and where to put it
    Wait,
    Cancel,  // Undo move - return unit to original position
}

/// Drop one passenger. Advance Wars lets a transport unload all its passengers
/// in one action: while it still carries more, the menu stays open (without
/// undo); dropping the last one ends its turn.
fn unload(
    events: &mut ActionEvents,
    units: &mut Query<(&mut Unit, Option<&FactionMember>, Option<&GridPosition>)>,
    pending_action: &mut PendingAction,
    turn_state: &mut TurnState,
    transport: Entity,
    cargo_index: usize,
    position: (i32, i32),
) {
    events.unload.write(UnloadEvent { transport, cargo_index, position });
    let Ok((mut unit, _, _)) = units.get_mut(transport) else { return };
    if unit.cargo.len() > 1 {
        pending_action.original_position = None;
        return;
    }
    unit.exhausted = true;
    pending_action.unit = None;
    pending_action.targets.clear();
    pending_action.can_capture = false;
    pending_action.capture_tile = None;
    pending_action.can_join = false;
    pending_action.join_target = None;
    turn_state.phase = TurnPhase::Select;
}

/// Resource to track action menu keyboard navigation
#[derive(Resource, Default)]
pub struct ActionMenuState {
    /// Currently selected menu item index
    pub selected_index: usize,
    /// List of available actions this frame
    pub actions: Vec<MenuAction>,
    /// Whether the menu was just opened (to reset selection)
    pub just_opened: bool,
}

/// SystemParam bundle for action event writers (reduces parameter count)
#[derive(SystemParam)]
pub struct ActionEvents<'w> {
    pub attack: MessageWriter<'w, AttackEvent>,
    pub capture: MessageWriter<'w, CaptureEvent>,
    pub join: MessageWriter<'w, JoinEvent>,
    pub resupply: MessageWriter<'w, ResupplyEvent>,
    pub load: MessageWriter<'w, LoadEvent>,
    pub unload: MessageWriter<'w, UnloadEvent>,
    pub cancel_move: MessageWriter<'w, CancelMoveEvent>,
}

/// SystemParam bundle for action menu read-only context (reduces parameter count)
#[derive(SystemParam)]
pub struct ActionMenuContext<'w> {
    pub game_result: Res<'w, GameResult>,
    pub commanders: Res<'w, Commanders>,
    pub weather: Res<'w, Weather>,
    pub game_data: Res<'w, GameData>,
    pub battle_config: Res<'w, BattleConfig>,
}

/// Resource to track battle setup (CO + Map selection)
#[derive(Resource)]
pub struct BattleSetupState {
    pub needs_setup: bool,
    pub player_faction: Faction,
    pub player_co: Option<CommanderId>,
    pub selected_map: MapId,
}

impl Default for BattleSetupState {
    fn default() -> Self {
        Self {
            needs_setup: false,
            player_faction: Faction::Western,
            player_co: Some(CommanderId::Kira),  // Pre-select first CO
            selected_map: MapId::default(),
        }
    }
}

/// Edit mode for map editor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorMode {
    #[default]
    Terrain,
    Units,
    Properties,
}

/// Resource to track map editor state
#[derive(Resource)]
pub struct EditorState {
    /// The map being edited
    pub map: MapData,
    /// Current edit mode
    pub mode: EditorMode,
    /// Selected terrain brush
    pub selected_terrain: Terrain,
    /// Selected unit type for placement
    pub selected_unit: UnitType,
    /// Selected faction for units/properties
    pub selected_faction: Faction,
    /// Map name for saving
    pub map_name: String,
    /// Status message
    pub status: String,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            map: MapData::new("New Map", 12, 8),
            mode: EditorMode::Terrain,
            selected_terrain: Terrain::Grass,
            selected_unit: UnitType::Scout,
            selected_faction: Faction::Eastern,
            map_name: "custom_map".to_string(),
            status: String::new(),
        }
    }
}

/// Resource tracking hovered unit for tooltip display
#[derive(Resource, Default)]
pub struct HoveredUnit {
    pub entity: Option<Entity>,
    pub screen_pos: (f32, f32),
}

/// Resource to track selected tile for info panel (right-click or hover)
#[derive(Resource, Default)]
pub struct SelectedTile {
    pub position: Option<IVec2>,
    pub screen_pos: (f32, f32),
}

/// Resource to track if egui is ready (needs a frame to initialize)
#[derive(Resource, Default)]
pub struct EguiReady {
    pub frames: u32,
}

impl EguiReady {
    pub fn is_ready(&self) -> bool {
        self.frames >= 2  // Wait 2 frames for egui to fully initialize
    }
}

pub struct UiPlugin;

fn egui_is_ready(egui_ready: Res<EguiReady>) -> bool {
    egui_ready.is_ready()
}

fn increment_egui_frame_counter(mut egui_ready: ResMut<EguiReady>) {
    egui_ready.frames = egui_ready.frames.saturating_add(1);
}

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<BattleSetupState>()
            .init_resource::<EditorState>()
            .init_resource::<HoveredUnit>()
            .init_resource::<SelectedTile>()
            .init_resource::<ActionMenuState>()
            .init_resource::<EguiReady>()
            .init_resource::<hud::UnitScreenPositions>()
            .init_resource::<UiScale>()
            .init_resource::<PortraitTextures>()
            .init_resource::<UnitTextures>()
            .add_systems(Update, increment_egui_frame_counter)
            .add_systems(PreUpdate, update_modal_state)
            .add_systems(Update, scale_ui_to_window)
            .add_systems(Update, menu_key_sounds)
            .add_systems(EguiPrimaryContextPass, ui_sounds)
            .add_systems(EguiPrimaryContextPass, (theme::apply_theme, register_portraits, register_unit_textures, boot::register_brand_textures))
            .add_systems(Startup, boot::load_brand_textures)
            .add_systems(EguiPrimaryContextPass, (
                boot::draw_boot.run_if(in_state(GameState::Menu)),
                menu::draw_main_menu.run_if(in_state(GameState::Menu)),
                setup::draw_battle_setup.run_if(in_state(GameState::Battle)),
                hud::draw_weather_overlay.run_if(in_state(GameState::Battle)),
                hud::draw_unit_tokens.run_if(in_state(GameState::Battle)),
                hud::draw_hud.run_if(in_state(GameState::Battle)),
                hud::draw_menu_button.run_if(in_state(GameState::Battle)),
                hud::draw_power_cut_in.run_if(in_state(GameState::Battle)),
                hud::draw_turn_banner.run_if(in_state(GameState::Battle)),
                map_menu::draw_map_menu.run_if(in_state(GameState::Battle)),
                hud::draw_inspect_card.run_if(in_state(GameState::Battle)),
                production::draw_production_menu.run_if(in_state(GameState::Battle)),
                overlays::draw_game_over.run_if(in_state(GameState::Battle)),
                editor::draw_editor.run_if(in_state(GameState::Editor)),
            ).run_if(egui_is_ready))
            // Action menu registered separately
            .add_systems(EguiPrimaryContextPass,
                draw_action_menu.run_if(in_state(GameState::Battle)).run_if(egui_is_ready)
            )
            // Keep non-egui systems in Update
            .add_systems(Update, (
                handle_ingame_menu_input.run_if(in_state(GameState::Battle)),
                handle_fog_toggle.run_if(in_state(GameState::Battle)),
                track_hovered_unit.run_if(in_state(GameState::Battle)),
                track_hovered_tile.run_if(in_state(GameState::Battle)),
            ))
            .add_systems(OnEnter(GameState::Battle), trigger_battle_setup)
            .add_systems(OnExit(GameState::Battle), overlays::cleanup_battle_on_exit)
            .add_systems(OnEnter(GameState::Editor), editor::setup_editor);
    }
}

/// egui texture ids for commander portrait sprites (empty until any are loaded)
#[derive(Resource, Default)]
pub struct PortraitTextures {
    textures: std::collections::HashMap<CommanderId, egui::TextureId>,
    figures: std::collections::HashMap<CommanderId, egui::TextureId>,
    registered: bool,
}

impl PortraitTextures {
    pub fn get(&self, id: CommanderId) -> Option<egui::TextureId> {
        self.textures.get(&id).copied()
    }

    /// The commander's full-body art (80x144), where it exists
    pub fn figure(&self, id: CommanderId) -> Option<egui::TextureId> {
        self.figures.get(&id).copied()
    }
}

/// Hand loaded portrait sprites to egui once sprite loading has finished
fn register_portraits(
    mut contexts: EguiContexts,
    sprites: Res<crate::game::SpriteAssets>,
    images: Res<Assets<Image>>,
    mut portraits: ResMut<PortraitTextures>,
) {
    if portraits.registered || !sprites.loading_complete {
        return;
    }
    for &id in CommanderId::all() {
        if let Some(handle) = sprites.portrait(&images, id) {
            let texture = contexts.add_image(bevy_egui::EguiTextureHandle::Strong(handle));
            portraits.textures.insert(id, texture);
        }
        if let Some(handle) = sprites.figure(&images, id) {
            let texture = contexts.add_image(bevy_egui::EguiTextureHandle::Strong(handle));
            portraits.figures.insert(id, texture);
        }
    }
    portraits.registered = true;
}

/// Unit map sprites handed to egui, for menus like the build list
#[derive(Resource, Default)]
pub struct UnitTextures {
    /// Texture and number of animation frames in its strip
    textures: std::collections::HashMap<(crate::game::Faction, crate::game::UnitType), (egui::TextureId, u32)>,
    registered: bool,
}

impl UnitTextures {
    pub fn get(&self, faction: crate::game::Faction, unit_type: crate::game::UnitType) -> Option<(egui::TextureId, u32)> {
        self.textures.get(&(faction, unit_type)).copied()
    }
}

/// Hand loaded unit sprites to egui once sprite loading has finished
fn register_unit_textures(
    mut contexts: EguiContexts,
    sprites: Res<crate::game::SpriteAssets>,
    images: Res<Assets<Image>>,
    mut units: ResMut<UnitTextures>,
) {
    if units.registered || !sprites.loading_complete {
        return;
    }
    for &faction in crate::game::Faction::all() {
        for &unit_type in crate::game::UnitType::all() {
            let Some(handle) = sprites.unit_sprite(&images, faction, unit_type) else { continue };
            let frames = images.get(&handle).map_or(1, crate::game::frame_count);
            let texture = contexts.add_image(bevy_egui::EguiTextureHandle::Strong(handle));
            units.textures.insert((faction, unit_type), (texture, frames));
        }
    }
    units.registered = true;
}

/// Tell the board whether a modal screen currently owns input
fn update_modal_state(
    mut modal: ResMut<crate::game::ModalOpen>,
    state: Res<State<GameState>>,
    setup: Res<BattleSetupState>,
    production: Res<ProductionState>,
    game_result: Res<GameResult>,
    map_menu: Res<crate::game::MapMenu>,
    banner: Res<crate::game::TurnBanner>,
) {
    let open = *state.get() != GameState::Battle
        || setup.needs_setup
        || production.active
        || map_menu.open
        || banner.playing()
        || game_result.game_over;
    if modal.0 != open {
        modal.0 = open;
    }
}

/// The UI's size relative to its 1280x720 design. Small windows (a phone in
/// landscape) shrink it, never below `MIN_UI_SCALE` so text and tap targets
/// stay usable; big windows keep it at full size. Positions from the 3D
/// camera (logical pixels) divide by this to land in UI points.
#[derive(Resource, Clone, Copy, Debug)]
pub struct UiScale(pub f32);

impl Default for UiScale {
    fn default() -> Self {
        Self(1.0)
    }
}

const MIN_UI_SCALE: f32 = 0.72;

fn scale_ui_to_window(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut contexts: Query<&mut bevy_egui::EguiContextSettings>,
    mut scale: ResMut<UiScale>,
) {
    let Ok(window) = windows.single() else { return };
    let fit = (window.height() / 720.0).min(window.width() / 1280.0);
    let wanted = fit.clamp(MIN_UI_SCALE, 1.0);
    if (scale.0 - wanted).abs() > 1e-3 || scale.is_added() {
        scale.0 = wanted;
    }
    for mut settings in &mut contexts {
        if (settings.scale_factor - scale.0).abs() > 1e-3 {
            settings.scale_factor = scale.0;
        }
    }
}

/// Hand the sounds queued by theme widgets to the audio plugin
fn ui_sounds(mut contexts: EguiContexts, mut play: MessageWriter<crate::game::PlaySfx>) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    for id in theme::take_sounds(ctx) {
        play.write(crate::game::PlaySfx(id));
    }
}

/// While a menu is up, keys and the pad tick, confirm and back out audibly.
/// (Menu rows also tick when their selection changes; repeats in one frame
/// play once.)
fn menu_key_sounds(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    state: Res<State<GameState>>,
    modal: Res<crate::game::ModalOpen>,
    turn_state: Res<TurnState>,
    banner: Res<crate::game::TurnBanner>,
    mut play: MessageWriter<crate::game::PlaySfx>,
) {
    let in_menu = match state.get() {
        GameState::Editor => false,
        GameState::Battle => (modal.0 || turn_state.phase == TurnPhase::Action) && !banner.playing(),
        _ => true,
    };
    if !in_menu {
        return;
    }
    let pad = |b: GamepadButton| gamepads.iter().any(|g| g.just_pressed(b));
    let any_key = |keys: &[KeyCode]| keys.iter().any(|k| keyboard.just_pressed(*k));
    if any_key(&[KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::KeyW, KeyCode::KeyS])
        || pad(GamepadButton::DPadUp) || pad(GamepadButton::DPadDown) || pad(GamepadButton::DPadLeft) || pad(GamepadButton::DPadRight)
    {
        play.write(crate::game::PlaySfx("menu_move"));
    }
    if any_key(&[KeyCode::Enter, KeyCode::Space]) || pad(GamepadButton::South) {
        play.write(crate::game::PlaySfx("menu_confirm"));
    }
    if any_key(&[KeyCode::Escape, KeyCode::Backspace]) || pad(GamepadButton::East) {
        play.write(crate::game::PlaySfx("menu_back"));
    }
}

/// Trigger battle setup when entering battle state
fn trigger_battle_setup(mut setup_state: ResMut<BattleSetupState>) {
    // Faction, commander and map choices carry over from the last battle
    setup_state.needs_setup = true;
}

/// Handle ESC key to toggle in-game menu
fn handle_ingame_menu_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut pending_action: ResMut<PendingAction>,
    mut highlights: ResMut<MovementHighlights>,
    setup_state: Res<BattleSetupState>,
    game_result: Res<GameResult>,
    production: Res<ProductionState>,
    mut map_menu: ResMut<crate::game::MapMenu>,
) {
    // Don't handle menu during setup or victory screen, or while another menu
    // is open (its own Esc closes it)
    if setup_state.needs_setup || game_result.game_over || production.active || map_menu.open {
        return;
    }

    // Esc, or a right click for mouse players
    if keyboard.just_pressed(KeyCode::Escape) || mouse.just_pressed(MouseButton::Right) {
        // If there's an active action/selection, cancel it first
        if pending_action.unit.is_some() || highlights.selected_unit.is_some() {
            pending_action.unit = None;
            pending_action.targets.clear();
            pending_action.can_capture = false;
            pending_action.capture_tile = None;
            pending_action.can_join = false;
            pending_action.join_target = None;
            highlights.selected_unit = None;
            highlights.tiles.clear();
            highlights.attack_targets.clear();
        } else {
            // Otherwise open the map menu (it closes itself on Esc)
            map_menu.open = true;
        }
    }
}

/// Draw the action menu when a unit has moved and can attack
/// Supports keyboard navigation with arrow keys/WASD and Enter/Space to confirm
fn draw_action_menu(
    mut contexts: EguiContexts,
    mut pending_action: ResMut<PendingAction>,
    mut turn_state: ResMut<TurnState>,
    mut units: Query<(&mut Unit, Option<&FactionMember>, Option<&GridPosition>)>,
    tiles: Query<&Tile>,
    mut events: ActionEvents,
    map: Res<GameMap>,
    ctx_res: ActionMenuContext,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut menu_state: ResMut<ActionMenuState>,
    mut cursor: ResMut<GridCursor>,
    screen_positions: Res<hud::UnitScreenPositions>,
) {
    // Unpack context for convenience
    let game_data = &ctx_res.game_data;

    // Don't show if game is over
    if ctx_res.game_result.game_over {
        return;
    }

    // Only show during Action phase
    if turn_state.phase != TurnPhase::Action {
        // Reset menu state when not in action phase
        menu_state.actions.clear();
        menu_state.selected_index = 0;
        menu_state.just_opened = true;
        return;
    }

    let Some(acting_entity) = pending_action.unit else {
        return;
    };

    // Get attacker info for damage calculation
    let attacker_info = units.get(acting_entity).ok().map(|(unit, _, pos)| {
        (unit.clone(), pos.map(|p| (p.x, p.y)).unwrap_or((0, 0)))
    });

    let Some((attacker_unit, attacker_pos)) = attacker_info else {
        return;
    };

    // Reset selection when menu just opened and skip input this frame
    // (prevents Space/Enter from movement confirmation being read as menu confirm)
    let skip_input = menu_state.just_opened;
    if menu_state.just_opened {
        menu_state.selected_index = 0;
        menu_state.just_opened = false;
    }

    // Handle keyboard navigation (skip if menu just opened this frame)
    let nav_up = !skip_input && (keyboard.just_pressed(KeyCode::ArrowUp) || keyboard.just_pressed(KeyCode::KeyW));
    let nav_down = !skip_input && (keyboard.just_pressed(KeyCode::ArrowDown) || keyboard.just_pressed(KeyCode::KeyS));
    let nav_confirm = !skip_input && (keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::Space));
    let nav_cancel = !skip_input && keyboard.just_pressed(KeyCode::Escape);

    // Get CO bonuses for damage calculation
    let attacker_co = ctx_res.commanders.get_bonuses(turn_state.current_faction, game_data);
    // Defender faction - opposite of current
    let defender_faction = ctx_res.battle_config.next_faction(turn_state.current_faction);
    let defender_co = ctx_res.commanders.get_bonuses(defender_faction, game_data);

    // Collect target info with damage estimates (min-max range due to luck)
    let target_info: Vec<_> = pending_action.targets.iter()
        .filter_map(|&entity| {
            units.get(entity).ok().map(|(unit, _faction, pos)| {
                let pos_xy = pos.map(|p| (p.x, p.y)).unwrap_or((0, 0));

                // Calculate damage estimate range (with CO bonuses and weather)
                let defender_terrain = map.get(pos_xy.0, pos_xy.1).unwrap_or(Terrain::Grass);
                let attacker_terrain = map.get(attacker_pos.0, attacker_pos.1).unwrap_or(Terrain::Grass);
                let (damage_min, damage_max) = estimate_damage(&attacker_unit, attacker_terrain, &unit, defender_terrain,
                    &attacker_co, &defender_co, &ctx_res.weather, &ctx_res.game_data);

                // Calculate counter-attack damage range (same rule as combat: the
                // damage table allows it and the attacker is within the defender's range)
                let (counter_min_range, counter_max_range) = game_data.unit_stats(unit.unit_type).attack_range;
                let distance = ((pos_xy.0 - attacker_pos.0).abs() + (pos_xy.1 - attacker_pos.1).abs()) as u32;
                let can_counter = game_data.can_damage(unit.unit_type, attacker_unit.unit_type)
                    && distance >= counter_min_range && distance <= counter_max_range;
                let counter_damage = if can_counter {
                    // Use average damage (midpoint) for counter calculation
                    let avg_damage = (damage_min + damage_max) / 2;
                    let defender_hp_after = (unit.hp - avg_damage).max(0);
                    if defender_hp_after > 0 {
                        // Create a temporary unit with reduced HP for counter calculation
                        let mut temp_defender = unit.clone();
                        temp_defender.hp = defender_hp_after;
                        let (counter_min, counter_max) = estimate_damage(&temp_defender, defender_terrain, &attacker_unit, attacker_terrain,
                            &defender_co, &attacker_co, &ctx_res.weather, &ctx_res.game_data);
                        let counter = defender_co.counter();
                        Some(((counter_min as f32 * counter) as i32, (counter_max as f32 * counter) as i32))
                    } else {
                        None // Defender will be destroyed, no counter
                    }
                } else {
                    None // Defender can't counter (indirect or no attack)
                };

                (
                    entity,
                    game_data.unit_name(unit.unit_type).to_string(),
                    unit.hp,
                    (damage_min, damage_max),
                    counter_damage,
                )
            })
        })
        .collect();

    // Track which action was taken
    let mut attack_target: Option<Entity> = None;
    let mut capture_clicked = false;
    let mut join_clicked = false;
    let mut resupply_clicked = false;
    let mut load_target: Option<(i32, i32)> = None;  // Position of transport to load into
    let mut unload_position: Option<(usize, (i32, i32))> = None;
    let mut wait_clicked = false;
    let mut cancel_clicked = false;

    // Check if this is a Supplier unit that can resupply
    let is_supplier = attacker_unit.unit_type == UnitType::Supplier;

    // AW boarding: the unit moved onto a friendly transport with room for it.
    // Then Load (or undo) is the only choice
    let units_here = units.iter().filter(|(_, _, p)| p.is_some_and(|p| (p.x, p.y) == attacker_pos)).count();
    let adjacent_transport_info: Vec<((i32, i32), String)> = if units_here >= 2 {
        units.iter()
            .filter(|(_, f, p)| p.is_some_and(|p| (p.x, p.y) == attacker_pos) && f.is_some_and(|f| f.faction == turn_state.current_faction))
            .find(|(u, _, _)| u.can_carry(attacker_unit.unit_type, game_data))
            .map(|(u, _, _)| vec![(attacker_pos, game_data.unit_name(u.unit_type).to_string())])
            .unwrap_or_default()
    } else {
        vec![]
    };
    let boarding = !adjacent_transport_info.is_empty();
    let is_transport_with_cargo = attacker_unit.is_transport(game_data) && attacker_unit.has_cargo() && !boarding;
    let is_supplier = is_supplier && !boarding;

    // Unload options: each passenger onto each free, passable neighbouring tile
    let unload_positions: Vec<(usize, (i32, i32))> = if is_transport_with_cargo {
        let occupied: std::collections::HashSet<(i32, i32)> = units.iter()
            .filter_map(|(_, _, pos)| pos.map(|p| (p.x, p.y)))
            .collect();
        let neighbours = [
            (attacker_pos.0 - 1, attacker_pos.1),
            (attacker_pos.0 + 1, attacker_pos.1),
            (attacker_pos.0, attacker_pos.1 - 1),
            (attacker_pos.0, attacker_pos.1 + 1),
        ];
        attacker_unit.cargo.iter().enumerate()
            .flat_map(|(i, cargo)| {
                let class = game_data.unit_class(cargo.unit_type);
                neighbours.iter()
                    .filter(|&&(x, y)| !occupied.contains(&(x, y))
                        && map.get(x, y).is_some_and(|t| game_data.is_passable(t, class)))
                    .map(move |&pos| (i, pos))
                    .collect::<Vec<_>>()
            })
            .collect()
    } else {
        vec![]
    };

    // Get capture info if applicable
    let capture_info = if pending_action.can_capture && !boarding {
        pending_action.capture_tile.and_then(|tile_entity| {
            tiles.get(tile_entity).ok().map(|tile| {
                let required = game_data.terrain_capture_points(tile.terrain) as i32;
                let progress = tile.capture_progress;
                (game_data.terrain_name(tile.terrain), progress, required)
            })
        })
    } else {
        None
    };

    // Build list of available actions for keyboard navigation
    menu_state.actions.clear();

    // While boarding, the only choices are Load and undo
    let target_info = if boarding { Vec::new() } else { target_info };
    let can_join = pending_action.can_join && !boarding;

    // Add attack targets
    for (entity, _, _, _, _) in &target_info {
        menu_state.actions.push(MenuAction::Attack(*entity));
    }

    // Add capture if available
    if pending_action.can_capture {
        menu_state.actions.push(MenuAction::Capture);
    }

    // Add join if available
    if can_join {
        menu_state.actions.push(MenuAction::Join);
    }

    // Add resupply for suppliers
    if is_supplier {
        menu_state.actions.push(MenuAction::Resupply);
    }

    // Add load options
    for (pos, _) in &adjacent_transport_info {
        menu_state.actions.push(MenuAction::Load(*pos));
    }

    // Add unload options
    for &(i, pos) in &unload_positions {
        menu_state.actions.push(MenuAction::Unload(i, pos));
    }

    if !boarding {
        menu_state.actions.push(MenuAction::Wait);
    }

    // Cancel (undo move) - only if unit actually moved
    if pending_action.original_position.is_some() {
        menu_state.actions.push(MenuAction::Cancel);
    }

    let action_count = menu_state.actions.len();

    // Handle keyboard navigation
    if nav_up && menu_state.selected_index > 0 {
        menu_state.selected_index -= 1;
    }
    if nav_down && menu_state.selected_index < action_count.saturating_sub(1) {
        menu_state.selected_index += 1;
    }

    // Clamp selection to valid range
    if menu_state.selected_index >= action_count {
        menu_state.selected_index = action_count.saturating_sub(1);
    }

    // Determine which action to execute (keyboard confirm or will be set by button click)
    let confirmed_action = if nav_confirm && !menu_state.actions.is_empty() {
        Some(menu_state.actions[menu_state.selected_index].clone())
    } else {
        None
    };

    let Ok(ctx) = contexts.ctx_mut() else { return };

    // Sit beside the acting unit (Advance Wars style), kept on screen
    let screen = ctx.content_rect();
    let anchor = screen_positions.0.get(&acting_entity).copied()
        .map(|p| egui::pos2(p.x + 44.0, p.y - 60.0))
        .unwrap_or(egui::pos2(screen.center().x + 60.0, screen.center().y - 120.0));
    let menu_width = 300.0;
    let pos = egui::pos2(
        anchor.x.min(screen.right() - menu_width - 16.0).max(16.0),
        anchor.y.clamp(16.0, (screen.bottom() - 420.0).max(16.0)),
    );

    let selected = menu_state.selected_index;
    let mut hovered_index: Option<usize> = None;
    egui::Area::new(egui::Id::new("action_menu"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            theme::card().show(ui, |ui| {
                ui.set_width(menu_width - 28.0);
                ui.label(theme::display(game_data.unit_name(attacker_unit.unit_type), 20.0).color(theme::WOOD_DARK));
                ui.add_space(2.0);
                let mut idx = 0usize;
                use icons::Icon;
                let mut row = |ui: &mut egui::Ui, icon: Icon, label: &str, detail: Option<&str>, tone: theme::Tone| -> bool {
                    let response = theme::menu_item_icon(ui, Some(icon), label, detail, idx == selected, true, tone);
                    if theme::pointer_selects(ui, &response) {
                        hovered_index = Some(idx);
                    }
                    idx += 1;
                    response.clicked()
                };

                for (entity, name, hp, (damage_min, damage_max), counter_damage) in &target_info {
                    let deal = if damage_min == damage_max { format!("{damage_min}%") } else { format!("{damage_min}–{damage_max}%") };
                    let outcome = if *damage_min >= *hp {
                        "  · destroys".to_string()
                    } else if *damage_max >= *hp {
                        "  · may destroy".to_string()
                    } else {
                        match counter_damage {
                            Some((cmin, cmax)) if cmin == cmax => format!("  · takes {cmin}% back"),
                            Some((cmin, cmax)) => format!("  · takes {cmin}–{cmax}% back"),
                            None => "  · no counter".to_string(),
                        }
                    };
                    let detail = format!("Deals {deal}{outcome}");
                    if row(ui, Icon::Sword, &format!("Attack {name}"), Some(&detail), theme::Tone::Normal) {
                        attack_target = Some(*entity);
                    }
                }
                if let Some((terrain_name, progress, required)) = &capture_info {
                    // Advance Wars counts down the points left to capture
                    let detail = format!("{} of {} left", required - progress, required);
                    if row(ui, Icon::Flag, &format!("Capture {terrain_name}"), Some(&detail), theme::Tone::Normal) {
                        capture_clicked = true;
                    }
                }
                if can_join && row(ui, Icon::Plus, "Join", Some("Merge HP, stamina and ammo"), theme::Tone::Normal) {
                    join_clicked = true;
                }
                if is_supplier && row(ui, Icon::Crate, "Resupply", Some("Refill adjacent allies"), theme::Tone::Normal) {
                    resupply_clicked = true;
                }
                for (transport_pos, transport_name) in &adjacent_transport_info {
                    if row(ui, Icon::LoadIn, &format!("Load into {transport_name}"), None, theme::Tone::Normal) {
                        load_target = Some(*transport_pos);
                    }
                }
                for &(i, (ux, uy)) in &unload_positions {
                    let terrain = map.get(ux, uy).unwrap_or(Terrain::Grass);
                    let dir = match (ux - attacker_pos.0, uy - attacker_pos.1) {
                        (-1, 0) => "west", (1, 0) => "east", (0, -1) => "north", _ => "south",
                    };
                    let label = format!("Unload {}", game_data.unit_name(attacker_unit.cargo[i].unit_type));
                    let detail = format!("To the {dir} · {}", game_data.terrain_name(terrain));
                    if row(ui, Icon::UnloadOut, &label, Some(&detail), theme::Tone::Normal) {
                        unload_position = Some((i, (ux, uy)));
                    }
                }
                if !boarding && row(ui, Icon::Hourglass, "Wait", None, theme::Tone::Normal) {
                    wait_clicked = true;
                }
                if pending_action.original_position.is_some() && row(ui, Icon::Undo, "Undo move", None, theme::Tone::Normal) {
                    cancel_clicked = true;
                }
            });
        });
    if let Some(i) = hovered_index {
        menu_state.selected_index = i;
    }

    // Process action outside the UI closure
    if let Some(target) = attack_target {
        events.attack.write(AttackEvent {
            attacker: acting_entity,
            defender: target,
        });

        if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
            unit.attacked = true;
            unit.exhausted = true;
        }

        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        pending_action.can_join = false;
        pending_action.join_target = None;
        turn_state.phase = TurnPhase::Select;
    } else if capture_clicked {
        if let Some(tile_entity) = pending_action.capture_tile {
            events.capture.write(CaptureEvent {
                unit: acting_entity,
                tile: tile_entity,
            });

            if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
                unit.attacked = true; // Capturing ends the unit's turn
                unit.exhausted = true;
            }

            pending_action.unit = None;
            pending_action.targets.clear();
            pending_action.can_capture = false;
            pending_action.capture_tile = None;
            pending_action.can_join = false;
            pending_action.join_target = None;
            turn_state.phase = TurnPhase::Select;
        }
    } else if join_clicked {
        if let Some(join_target) = pending_action.join_target {
            events.join.write(JoinEvent {
                source: acting_entity,
                target: join_target,
            });

            // The source unit will be despawned by the join system
            // No need to mark as attacked since it will be gone

            pending_action.unit = None;
            pending_action.targets.clear();
            pending_action.can_capture = false;
            pending_action.capture_tile = None;
            pending_action.can_join = false;
            pending_action.join_target = None;
            turn_state.phase = TurnPhase::Select;
        }
    } else if resupply_clicked {
        // Send resupply event - the system will handle finding and resupplying adjacent units
        events.resupply.write(ResupplyEvent {
            supplier: acting_entity,
        });

        if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
            unit.exhausted = true;
        }

        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        pending_action.can_join = false;
        pending_action.join_target = None;
        turn_state.phase = TurnPhase::Select;
    } else if let Some(transport_pos) = load_target {
        // Send load event with transport position - the handler will find the transport
        events.load.write(LoadEvent {
            transport_pos,
            passenger: acting_entity,
        });

        // Mark passenger as exhausted (will be hidden inside transport anyway)
        if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
            unit.exhausted = true;
        }

        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        pending_action.can_join = false;
        pending_action.join_target = None;
        turn_state.phase = TurnPhase::Select;
    } else if let Some((cargo_index, pos)) = unload_position {
        unload(&mut events, &mut units, &mut pending_action, &mut turn_state, acting_entity, cargo_index, pos);
    } else if wait_clicked {
        // Mark unit as exhausted when waiting
        if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
            unit.exhausted = true;
        }

        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        pending_action.can_join = false;
        pending_action.join_target = None;
        turn_state.phase = TurnPhase::Select;
    }

    // Handle keyboard confirmation
    if let Some(action) = confirmed_action {
        match action {
            MenuAction::Attack(target) => {
                events.attack.write(AttackEvent {
                    attacker: acting_entity,
                    defender: target,
                });
                if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
                    unit.attacked = true;
                    unit.exhausted = true;
                }
                pending_action.unit = None;
                pending_action.targets.clear();
                pending_action.can_capture = false;
                pending_action.capture_tile = None;
                pending_action.can_join = false;
                pending_action.join_target = None;
                turn_state.phase = TurnPhase::Select;
            }
            MenuAction::Capture => {
                if let Some(tile_entity) = pending_action.capture_tile {
                    events.capture.write(CaptureEvent {
                        unit: acting_entity,
                        tile: tile_entity,
                    });
                    if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
                        unit.attacked = true;
                        unit.exhausted = true;
                    }
                    pending_action.unit = None;
                    pending_action.targets.clear();
                    pending_action.can_capture = false;
                    pending_action.capture_tile = None;
                    pending_action.can_join = false;
                    pending_action.join_target = None;
                    turn_state.phase = TurnPhase::Select;
                }
            }
            MenuAction::Join => {
                if let Some(join_target) = pending_action.join_target {
                    events.join.write(JoinEvent {
                        source: acting_entity,
                        target: join_target,
                    });
                    pending_action.unit = None;
                    pending_action.targets.clear();
                    pending_action.can_capture = false;
                    pending_action.capture_tile = None;
                    pending_action.can_join = false;
                    pending_action.join_target = None;
                    turn_state.phase = TurnPhase::Select;
                }
            }
            MenuAction::Resupply => {
                events.resupply.write(ResupplyEvent {
                    supplier: acting_entity,
                });
                if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
                    unit.exhausted = true;
                }
                pending_action.unit = None;
                pending_action.targets.clear();
                pending_action.can_capture = false;
                pending_action.capture_tile = None;
                pending_action.can_join = false;
                pending_action.join_target = None;
                turn_state.phase = TurnPhase::Select;
            }
            MenuAction::Load(transport_pos) => {
                events.load.write(LoadEvent {
                    transport_pos,
                    passenger: acting_entity,
                });
                if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
                    unit.exhausted = true;
                }
                pending_action.unit = None;
                pending_action.targets.clear();
                pending_action.can_capture = false;
                pending_action.capture_tile = None;
                pending_action.can_join = false;
                pending_action.join_target = None;
                turn_state.phase = TurnPhase::Select;
            }
            MenuAction::Unload(cargo_index, pos) => {
                unload(&mut events, &mut units, &mut pending_action, &mut turn_state, acting_entity, cargo_index, pos);
            }
            MenuAction::Wait => {
                if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
                    unit.exhausted = true;
                }
                pending_action.unit = None;
                pending_action.targets.clear();
                pending_action.can_capture = false;
                pending_action.capture_tile = None;
                pending_action.can_join = false;
                pending_action.join_target = None;
                pending_action.original_position = None;
                turn_state.phase = TurnPhase::Select;
            }
            MenuAction::Cancel => {
                cancel_clicked = true;
            }
        }
    }

    // Handle Cancel (undo move - return unit to original position)
    if cancel_clicked {
        if let Some((orig_x, orig_y)) = pending_action.original_position {
            if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
                // Reset movement flags - unit can move again
                unit.moved = false;
                unit.attacked = false;
                unit.exhausted = false;
            }
            // Send event to update position (handled by separate system)
            events.cancel_move.write(CancelMoveEvent {
                unit: acting_entity,
                original_position: (orig_x, orig_y),
            });
            // Move cursor back to original position
            cursor.x = orig_x;
            cursor.y = orig_y;
            info!("Cancelled move, returning to ({}, {})", orig_x, orig_y);
        }
        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        pending_action.can_join = false;
        pending_action.join_target = None;
        pending_action.original_position = None;
        turn_state.phase = TurnPhase::Select;
    }


    // Handle cancel (ESC) - same as Wait
    if nav_cancel {
        if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
            unit.exhausted = true;
        }
        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        pending_action.can_join = false;
        pending_action.join_target = None;
        pending_action.original_position = None;
        turn_state.phase = TurnPhase::Select;
    }
}

/// Handle keyboard shortcut for fog toggle
fn handle_fog_toggle(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut fog: ResMut<FogOfWar>,
) {
    if keyboard.just_pressed(KeyCode::KeyF) {
        fog.enabled = !fog.enabled;
        info!("Fog of war: {}", if fog.enabled { "ON" } else { "OFF" });
    }
}

// ============================================================================
// UNIT TOOLTIP
// ============================================================================

/// Track which unit is under the mouse cursor
fn track_hovered_unit(
    pointer: Res<crate::game::BoardPointer>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    units: Query<(Entity, &GridPosition, &Unit, &FactionMember)>,
    map: Res<GameMap>,
    mut hovered: ResMut<HoveredUnit>,
    fog: Res<FogOfWar>,
    battle_config: Res<BattleConfig>,
) {
    let Ok((camera, camera_transform)) = cameras.single() else { return };

    let Some(cursor_pos) = pointer.position else {
        hovered.entity = None;
        return;
    };

    // Convert screen position to grid coordinates using ray-plane intersection
    let Some(grid_pos) = screen_to_grid(Some(cursor_pos), camera, camera_transform, &map) else {
        hovered.entity = None;
        return;
    };
    let grid_x = grid_pos.x;
    let grid_y = grid_pos.y;

    // Find unit at this position
    hovered.entity = None;
    for (entity, pos, _unit, faction) in units.iter() {
        if pos.x == grid_x && pos.y == grid_y {
            // Check fog of war - only show tooltip for visible units
            if fog.enabled && faction.faction != battle_config.player_faction && !fog.is_visible(pos.x, pos.y) {
                continue;
            }
            hovered.entity = Some(entity);
            hovered.screen_pos = (cursor_pos.x, cursor_pos.y);
            break;
        }
    }
}

/// Track hovered tile for terrain info panel (only when not hovering a unit)
fn track_hovered_tile(
    pointer: Res<crate::game::BoardPointer>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    map: Res<GameMap>,
    hovered_unit: Res<HoveredUnit>,
    mut selected_tile: ResMut<SelectedTile>,
    fog: Res<FogOfWar>,
) {
    // Don't track tile if hovering a unit (unit tooltip takes priority)
    if hovered_unit.entity.is_some() {
        selected_tile.position = None;
        return;
    }

    let Ok((camera, camera_transform)) = cameras.single() else { return };

    let Some(cursor_pos) = pointer.position else {
        selected_tile.position = None;
        return;
    };

    // Convert screen position to grid coordinates
    let Some(grid_pos) = screen_to_grid(Some(cursor_pos), camera, camera_transform, &map) else {
        selected_tile.position = None;
        return;
    };

    // Check fog of war - only show for visible/explored tiles
    if fog.enabled {
        let visibility = fog.get_visibility(grid_pos.x, grid_pos.y);
        if visibility == crate::game::TileVisibility::Unexplored {
            selected_tile.position = None;
            return;
        }
    }

    selected_tile.position = Some(grid_pos);
    selected_tile.screen_pos = (cursor_pos.x, cursor_pos.y);
}
