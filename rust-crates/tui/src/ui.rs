use crate::client::{
    AppSnapshot,
    OtherPlayerBets,
    PreviousGameSummary,
    VrfMode,
};
use color_eyre::eyre::{
    Result,
    eyre,
};
use crossterm::{
    event::{
        self,
        Event,
        KeyCode,
        KeyEventKind,
    },
    terminal::{
        disable_raw_mode,
        enable_raw_mode,
    },
};
use fuels::types::AssetId;
use ratatui::{
    prelude::*,
    widgets::*,
};
use std::{
    io::stdout,
    thread,
};
use strapped_contract::strapped_types as strapped;
use tokio::sync::mpsc;
use tracing::error;

pub enum UserEvent {
    Quit,
    NextRoll,
    PrevRoll,
    PlaceBetAmount(u64),
    Purchase,
    Roll,
    VRFInc,
    VRFDec,
    SetVrf(u64),
    OpenBetModal,
    OpenClaimModal,
    OpenVrfModal,
    Redraw,
    OpenShop,
    ConfirmShopPurchase {
        roll: strapped::Roll,
        modifier: strapped::Modifier,
    },
    OpenStrapBet,
    OpenStrapInventory,
    ConfirmStrapBet {
        strap: strapped::Strap,
        amount: u64,
    },
    ConfirmClaim {
        game_id: u32,
        enabled: Vec<(strapped::Roll, strapped::Modifier)>,
    },
}

#[derive(Debug)]
pub struct UiState {
    mode: Mode,
    prev_games: Vec<PreviousGameSummary>,
    current_vrf: u64,
    terminal: Option<Terminal<CrosstermBackend<std::io::Stdout>>>,
    shop_items: Vec<(
        strapped::Roll,
        strapped::Roll,
        strapped::Modifier,
        bool,
        bool,
        u64,
    )>,
    last_game_id: Option<u32>,
    owned_straps: Vec<(strapped::Strap, u64)>,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            mode: Mode::Normal,
            prev_games: Vec::new(),
            current_vrf: 0,
            terminal: None,
            shop_items: Vec::new(),
            last_game_id: None,
            owned_straps: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
enum Mode {
    #[default]
    Normal,
    BetModal(BetState),
    TableBetsModal(TableBetsState),
    ClaimModal(ClaimState),
    VrfModal(VrfState),
    ShopModal(ShopState),
    QuitModal,
    StrapBet(StrapBetState),
    StrapInventory(StrapInventoryState),
}

#[derive(Clone, Debug, Default)]
struct BetState {
    amount: u64,
}

#[derive(Clone, Debug, Default)]
struct TableBetsState;

#[derive(Clone, Debug, Default)]
struct ClaimState {
    game_idx: usize,
    mod_idx: usize,
    selected: Vec<(strapped::Roll, strapped::Modifier)>,
    focus: ClaimFocus,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum ClaimFocus {
    #[default]
    Games,
    Modifiers,
}

fn has_user_bets(g: &PreviousGameSummary) -> bool {
    g.cells
        .iter()
        .any(|c| c.chip_total > 0 || c.strap_total > 0)
}

fn roll_hit_after_bet(
    target_roll: &strapped::Roll,
    bet_roll_index: u32,
    rolls: &[strapped::Roll],
) -> bool {
    rolls
        .iter()
        .enumerate()
        .any(|(idx, r)| r == target_roll && bet_roll_index <= idx as u32)
}

fn has_claimable_bets(g: &PreviousGameSummary) -> bool {
    if g.claimed || g.rolls.is_empty() {
        return false;
    }
    for (roll, bets) in &g.bets_by_roll {
        for (_bet, _amt, bet_roll_index) in bets {
            if roll_hit_after_bet(roll, *bet_roll_index, &g.rolls) {
                return true;
            }
        }
    }
    false
}

fn claimable_games(prev_games: &[PreviousGameSummary]) -> Vec<PreviousGameSummary> {
    prev_games
        .iter()
        .filter(|g| has_claimable_bets(g))
        .cloned()
        .collect()
}

fn game_status_label(g: &PreviousGameSummary) -> &'static str {
    if has_claimable_bets(g) {
        "[unclaimed]"
    } else {
        "[nothing-to-claim]"
    }
}

fn prune_selected(cs: &mut ClaimState, g: &PreviousGameSummary) {
    cs.selected
        .retain(|(rr, mm)| g.modifiers.iter().any(|(gr, gm, _)| gr == rr && gm == mm));
    if cs.selected.is_empty() && !g.modifiers.is_empty() {
        cs.selected = default_claim_selection(g);
    }
    if g.modifiers.is_empty() {
        cs.focus = ClaimFocus::Games;
        cs.mod_idx = 0;
    } else {
        cs.mod_idx = cs.mod_idx.min(g.modifiers.len() - 1);
    }
}

fn default_claim_selection(
    g: &PreviousGameSummary,
) -> Vec<(strapped::Roll, strapped::Modifier)> {
    g.modifiers
        .iter()
        .map(|(r, m, _)| (r.clone(), m.clone()))
        .collect()
}

pub fn terminal_enter(state: &mut UiState) -> Result<()> {
    enable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    // Create a single persistent Terminal to preserve buffers across draws
    let backend = CrosstermBackend::new(stdout());
    let terminal = Terminal::new(backend)?;
    state.terminal = Some(terminal);
    Ok(())
}

pub fn terminal_exit() -> Result<()> {
    disable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    Ok(())
}

pub fn draw(state: &mut UiState, snap: &AppSnapshot) -> Result<()> {
    // keep cache of previous games for modal interactions
    state.prev_games = snap.previous_games.clone();
    state.current_vrf = snap.vrf_number;
    // If game changed, reset shop selection and update items
    let game_changed = state.last_game_id != Some(snap.current_game_id);
    state.shop_items = snap.modifier_triggers.clone();
    state.owned_straps = snap.owned_straps.clone();
    if game_changed {
        // reset selection index if currently in shop
        if let Mode::ShopModal(ref mut ss) = state.mode {
            ss.idx = 0;
        }
        state.last_game_id = Some(snap.current_game_id);
    }
    if let Some(mut term) = state.terminal.take() {
        term.draw(|f| ui(f, state, snap))?;
        state.terminal = Some(term);
    }
    Ok(())
}

pub type InputEventReceiver = mpsc::UnboundedReceiver<Event>;

pub fn input_event_stream() -> InputEventReceiver {
    let (tx, rx) = mpsc::unbounded_channel();
    thread::spawn(move || {
        loop {
            match event::read() {
                Ok(ev) => {
                    if tx.send(ev).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    error!(?err, "terminal input read failed");
                    break;
                }
            }
        }
    });
    rx
}

pub async fn next_raw_event(events: &mut InputEventReceiver) -> Result<Event> {
    events
        .recv()
        .await
        .ok_or_else(|| eyre!("terminal input channel closed"))
}

pub fn interpret_event(state: &mut UiState, event: Event) -> Option<UserEvent> {
    let Event::Key(k) = event else {
        return None;
    };
    if k.kind != KeyEventKind::Press {
        return None;
    }

    match &mut state.mode {
        Mode::BetModal(bs) => {
            return match k.code {
                KeyCode::Esc => {
                    state.mode = Mode::Normal;
                    Some(UserEvent::Redraw)
                }
                KeyCode::Enter => {
                    let amt = bs.amount;
                    state.mode = Mode::Normal;
                    Some(UserEvent::PlaceBetAmount(amt))
                }
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('+') => {
                    bs.amount = bs.amount.saturating_add(1);
                    Some(UserEvent::Redraw)
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('-') => {
                    bs.amount = bs.amount.saturating_sub(1);
                    Some(UserEvent::Redraw)
                }
                KeyCode::Backspace => {
                    bs.amount /= 10;
                    Some(UserEvent::Redraw)
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    let d = c.to_digit(10).unwrap() as u64;
                    bs.amount = bs.amount.saturating_mul(10).saturating_add(d);
                    Some(UserEvent::Redraw)
                }
                _ => None,
            };
        }
        Mode::TableBetsModal(_) => {
            return match k.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                    state.mode = Mode::Normal;
                    Some(UserEvent::Redraw)
                }
                _ => None,
            };
        }
        Mode::ClaimModal(cs) => {
            let claimable = claimable_games(&state.prev_games);
            if claimable.is_empty() {
                cs.game_idx = 0;
                cs.mod_idx = 0;
                cs.selected.clear();
                cs.focus = ClaimFocus::Games;
            } else {
                cs.game_idx = cs.game_idx.min(claimable.len() - 1);
                if let Some(g) = claimable.get(cs.game_idx) {
                    prune_selected(cs, g);
                }
            }
            return match k.code {
                KeyCode::Esc => {
                    state.mode = Mode::Normal;
                    Some(UserEvent::Redraw)
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    cs.focus = ClaimFocus::Games;
                    Some(UserEvent::Redraw)
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    if let Some(g) = claimable.get(cs.game_idx)
                        && !g.modifiers.is_empty()
                    {
                        cs.focus = ClaimFocus::Modifiers;
                        cs.mod_idx = cs.mod_idx.min(g.modifiers.len().saturating_sub(1));
                    }
                    Some(UserEvent::Redraw)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    match cs.focus {
                        ClaimFocus::Games => {
                            if cs.game_idx > 0 {
                                cs.game_idx -= 1;
                                if let Some(g) = claimable.get(cs.game_idx) {
                                    prune_selected(cs, g);
                                }
                            }
                        }
                        ClaimFocus::Modifiers => {
                            if cs.mod_idx > 0 {
                                cs.mod_idx -= 1;
                            }
                        }
                    }
                    Some(UserEvent::Redraw)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    match cs.focus {
                        ClaimFocus::Games => {
                            if cs.game_idx + 1 < claimable.len() {
                                cs.game_idx += 1;
                                if let Some(g) = claimable.get(cs.game_idx) {
                                    prune_selected(cs, g);
                                }
                            }
                        }
                        ClaimFocus::Modifiers => {
                            if let Some(g) = claimable.get(cs.game_idx)
                                && cs.mod_idx + 1 < g.modifiers.len()
                            {
                                cs.mod_idx += 1;
                            }
                        }
                    }
                    Some(UserEvent::Redraw)
                }
                KeyCode::Char(' ') => {
                    if cs.focus != ClaimFocus::Modifiers
                        && let Some(g) = claimable.get(cs.game_idx)
                        && !g.modifiers.is_empty()
                    {
                        cs.focus = ClaimFocus::Modifiers;
                    }
                    if let Some(g) = claimable.get(cs.game_idx)
                        && let Some((roll, modifier, _)) = g.modifiers.get(cs.mod_idx)
                    {
                        if let Some(pos) = cs
                            .selected
                            .iter()
                            .position(|(r, m)| r == roll && m == modifier)
                        {
                            cs.selected.remove(pos);
                        } else {
                            cs.selected.push((roll.clone(), modifier.clone()));
                        }
                    }
                    Some(UserEvent::Redraw)
                }
                KeyCode::Enter => {
                    if let Some(game) = claimable.get(cs.game_idx) {
                        let enabled = cs.selected.clone();
                        state.mode = Mode::Normal;
                        Some(UserEvent::ConfirmClaim {
                            game_id: game.game_id,
                            enabled,
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            };
        }
        Mode::VrfModal(vs) => {
            return match k.code {
                KeyCode::Esc => {
                    state.mode = Mode::Normal;
                    Some(UserEvent::Redraw)
                }
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('+') => {
                    vs.value = vs.value.saturating_add(1);
                    Some(UserEvent::Redraw)
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('-') => {
                    vs.value = vs.value.saturating_sub(1);
                    Some(UserEvent::Redraw)
                }
                KeyCode::Backspace => {
                    vs.value /= 10;
                    Some(UserEvent::Redraw)
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    let d = c.to_digit(10).unwrap() as u64;
                    vs.value = vs.value.saturating_mul(10).saturating_add(d);
                    Some(UserEvent::Redraw)
                }
                KeyCode::Enter => {
                    let value = vs.value;
                    state.mode = Mode::Normal;
                    Some(UserEvent::SetVrf(value))
                }
                _ => None,
            };
        }
        Mode::ShopModal(ss) => {
            return match k.code {
                KeyCode::Esc => {
                    state.mode = Mode::Normal;
                    Some(UserEvent::Redraw)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if ss.idx > 0 {
                        ss.idx -= 1;
                    }
                    Some(UserEvent::Redraw)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let max = state.shop_items.len().saturating_sub(1);
                    ss.idx = (ss.idx + 1).min(max);
                    Some(UserEvent::Redraw)
                }
                KeyCode::Enter => {
                    if let Some((_from, roll, modifier, triggered, purchased, _price)) =
                        state.shop_items.get(ss.idx).cloned()
                    {
                        if triggered && !purchased {
                            state.mode = Mode::Normal;
                            Some(UserEvent::ConfirmShopPurchase { roll, modifier })
                        } else {
                            Some(UserEvent::Redraw)
                        }
                    } else {
                        Some(UserEvent::Redraw)
                    }
                }
                _ => None,
            };
        }
        Mode::StrapBet(sb) => {
            return match k.code {
                KeyCode::Esc => {
                    state.mode = Mode::Normal;
                    Some(UserEvent::Redraw)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if sb.idx > 0 {
                        sb.idx -= 1;
                    }
                    Some(UserEvent::Redraw)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let max = state.owned_straps.len().saturating_sub(1);
                    sb.idx = (sb.idx + 1).min(max);
                    Some(UserEvent::Redraw)
                }
                KeyCode::Char('+') | KeyCode::Right => {
                    sb.amount = sb.amount.saturating_add(1);
                    Some(UserEvent::Redraw)
                }
                KeyCode::Char('-') | KeyCode::Left => {
                    sb.amount = sb.amount.saturating_sub(1).max(1);
                    Some(UserEvent::Redraw)
                }
                KeyCode::Enter => {
                    if let Some((strap, balance)) =
                        state.owned_straps.get(sb.idx).cloned()
                    {
                        let amount = sb.amount.min(balance);
                        state.mode = Mode::Normal;
                        Some(UserEvent::ConfirmStrapBet { strap, amount })
                    } else {
                        Some(UserEvent::Redraw)
                    }
                }
                _ => None,
            };
        }
        Mode::StrapInventory(si) => {
            return match k.code {
                KeyCode::Esc => {
                    state.mode = Mode::Normal;
                    Some(UserEvent::Redraw)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if si.idx > 0 {
                        si.idx -= 1;
                    }
                    Some(UserEvent::Redraw)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let max = strap_kind_catalog().len().saturating_sub(1);
                    si.idx = (si.idx + 1).min(max);
                    Some(UserEvent::Redraw)
                }
                KeyCode::Enter => {
                    state.mode = Mode::StrapBet(StrapBetState {
                        idx: si.idx,
                        amount: 1,
                    });
                    Some(UserEvent::OpenStrapBet)
                }
                _ => None,
            };
        }
        Mode::QuitModal => {
            return match k.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => Some(UserEvent::Quit),
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    state.mode = Mode::Normal;
                    Some(UserEvent::Redraw)
                }
                _ => None,
            };
        }
        Mode::Normal => {}
    }

    match k.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            state.mode = Mode::QuitModal;
            Some(UserEvent::Redraw)
        }
        KeyCode::Right | KeyCode::Char('l') => Some(UserEvent::NextRoll),
        KeyCode::Left | KeyCode::Char('h') => Some(UserEvent::PrevRoll),
        KeyCode::Char('b') => {
            state.mode = Mode::BetModal(BetState::default());
            Some(UserEvent::OpenBetModal)
        }
        KeyCode::Char('B') => {
            state.mode = Mode::TableBetsModal(TableBetsState::default());
            Some(UserEvent::Redraw)
        }
        KeyCode::Char('t') => {
            state.mode = Mode::StrapBet(StrapBetState::default());
            Some(UserEvent::OpenStrapBet)
        }
        KeyCode::Char('m') => Some(UserEvent::Purchase),
        KeyCode::Char('r') => Some(UserEvent::Roll),
        KeyCode::Char(']') => Some(UserEvent::VRFInc),
        KeyCode::Char('[') => Some(UserEvent::VRFDec),
        KeyCode::Char('/') => {
            state.mode = Mode::VrfModal(VrfState::default());
            Some(UserEvent::OpenVrfModal)
        }
        KeyCode::Char('s') => {
            state.mode = Mode::ShopModal(ShopState::default());
            Some(UserEvent::OpenShop)
        }
        KeyCode::Char('i') => {
            state.mode = Mode::StrapInventory(StrapInventoryState::default());
            Some(UserEvent::OpenStrapInventory)
        }
        KeyCode::Char('c') => {
            let mut cs = ClaimState::default();
            let claimable = claimable_games(&state.prev_games);
            if let Some(g) = claimable.first()
                && !g.modifiers.is_empty()
            {
                cs.selected = default_claim_selection(g);
            }
            state.mode = Mode::ClaimModal(cs);
            Some(UserEvent::OpenClaimModal)
        }
        _ => None,
    }
}

fn ui(f: &mut Frame, state: &UiState, snap: &AppSnapshot) {
    // Clear the whole frame to avoid leftover fragments
    f.render_widget(Clear, f.area());
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(18), // wallet + overview
            Constraint::Percentage(8),  // roll history
            Constraint::Percentage(32), // grid
            Constraint::Percentage(22), // shop + previous games
            Constraint::Percentage(20), // status/errors + help
        ])
        .split(f.area());

    draw_top(f, chunks[0], snap);
    // Roll history above grid
    draw_roll_history(f, chunks[1], snap);
    // Grid occupies its own row
    draw_grid(f, chunks[2], snap);
    // Shop (left) + Previous Games (right)
    draw_lower(f, state, chunks[3], snap);
    draw_bottom(f, chunks[4], snap);
    draw_modals(f, state, snap);
}

fn draw_top(f: &mut Frame, area: Rect, snap: &AppSnapshot) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);
    draw_wallet_panel(f, rows[0], snap);
    draw_overview_panel(f, rows[1], snap);
}

fn draw_grid(f: &mut Frame, area: Rect, snap: &AppSnapshot) {
    // Draw rolls 2..12 with 7 included
    let rolls = &snap.cells; // already ordered: [Two..Twelve], Seven in middle
    let cols = rolls.len() as u16; // 11
    let col_w = if cols > 0 {
        area.width / cols
    } else {
        area.width
    };
    let chip_label = snap.chip_asset_ticker.as_deref().unwrap_or("chips");
    for (i, cell) in rolls.iter().enumerate() {
        let c = i as u16;
        let rect = Rect::new(area.x + c * col_w, area.y, col_w, area.height);
        let selected = cell.roll == snap.selected_roll;
        let mut lines = Vec::new();
        lines.push(Line::from("Rewards:"));
        if cell.rewards.is_empty() {
            lines.push(Line::from(" None"));
        } else {
            for reward in &cell.rewards {
                let qty = if reward.count > 1 {
                    format!("{}x", reward.count)
                } else {
                    String::new()
                };
                lines.push(Line::from(format!(
                    "{}{}/{}{}",
                    qty,
                    strap_emoji(&reward.strap.kind),
                    reward.cost,
                    chip_label,
                )));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from("You:"));
        lines.extend(format_my_bet_lines(
            cell.chip_total,
            &cell.straps,
            chip_label,
        ));
        if !cell.table_bets.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from("Table:"));
            for entry in &cell.table_bets {
                for line in format_table_bet_lines(entry, chip_label) {
                    lines.push(Line::styled(
                        line,
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                }
            }
        }
        let label = Paragraph::new(lines);
        let mods = active_mods_emojis(&cell.roll, &snap.active_modifiers);
        let base = match cell.roll {
            strapped::Roll::Seven => String::from("Seven/RESET"),
            _ => format!("{:?}", cell.roll),
        };
        let title = if mods.is_empty() {
            base
        } else {
            format!("{} {}", base, mods)
        };
        let border_style =
            roll_border_style(&cell.roll, selected, &snap.active_modifiers);
        let title_style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(title, title_style));
        f.render_widget(&block, rect);
        let inner = block.inner(rect);
        f.render_widget(label, inner);
    }
}
// Roll history (current game) panel
fn draw_roll_history(f: &mut Frame, area: Rect, snap: &AppSnapshot) {
    let mut rh = vec![];
    if snap.roll_history.is_empty() {
        rh.push(Line::styled("None", Style::default().fg(Color::DarkGray)));
    } else {
        let items: Vec<String> = snap
            .roll_history
            .iter()
            .map(|r| format!("{:?}", r))
            .collect();
        rh.push(Line::from(items.join(" ")));
    }
    let roll_hist = Paragraph::new(rh)
        .block(Block::default().borders(Borders::ALL).title("Roll History"));
    f.render_widget(roll_hist, area);
}

// Lower area: Shop and Previous Games side-by-side
fn draw_lower(f: &mut Frame, state: &UiState, area: Rect, snap: &AppSnapshot) {
    let lower = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Shop on the left (all triggers; dim if not yet triggered)
    let mut shop_lines = Vec::new();
    if state.shop_items.is_empty() {
        shop_lines.push(Line::from("No modifiers available"));
    } else {
        // Show triggered first, then locked
        for (from, to, modifier, triggered, purchased, price) in &state.shop_items {
            let text = if *purchased {
                format!(
                    "{:?} {} - purchased ({price} chips)",
                    to,
                    modifier_emoji(modifier)
                )
            } else if *triggered {
                format!("{:?} {} - {price} chips", to, modifier_emoji(modifier))
            } else {
                format!(
                    "{:?} {} (Unlock by rolling {:?}) - {price} chips",
                    to,
                    modifier_emoji(modifier),
                    from
                )
            };
            let line = if *purchased {
                Line::styled(text, Style::default().fg(Color::Green))
            } else if *triggered {
                Line::from(text)
            } else {
                Line::styled(text, Style::default().fg(Color::DarkGray))
            };
            shop_lines.push(line);
        }
    }
    let shop = Paragraph::new(shop_lines)
        .block(Block::default().borders(Borders::ALL).title("Shop"));
    f.render_widget(shop, lower[0]);

    // Previous games on the right (latest at top)
    let mut prev_lines = vec![];
    if snap.previous_games.is_empty() {
        prev_lines.push(Line::from("None"));
    } else {
        for g in &snap.previous_games {
            let status = game_status_label(g);
            let has_bets = has_user_bets(g);
            // Rolls line
            if g.rolls.is_empty() {
                prev_lines.push(Line::from(format!(
                    "Game {} {} | Rolls: None",
                    g.game_id, status
                )));
            } else {
                let mut items: Vec<String> = Vec::new();
                for (idx, r) in g.rolls.iter().enumerate() {
                    // Append emojis for modifiers active at this roll index
                    let mut emo = String::new();
                    for (mr, mm, mi) in &g.modifiers {
                        if mr == r && (*mi as usize) <= idx {
                            let e = modifier_emoji(mm);
                            if !e.is_empty() {
                                emo.push_str(e);
                            }
                        }
                    }
                    items.push(if emo.is_empty() {
                        format!("{:?}", r)
                    } else {
                        format!("{:?}{}", r, emo)
                    });
                }
                prev_lines.push(Line::from(format!(
                    "Game {} {} | Rolls: {}",
                    g.game_id,
                    status,
                    items.join(" ")
                )));
            }
            if has_bets {
                // Bets list with indices
                prev_lines.push(Line::from("  Bets:"));
                for (roll, bets) in &g.bets_by_roll {
                    for (bet, amt, idx) in bets {
                        match bet {
                            strapped::Bet::Chip => prev_lines.push(Line::from(format!(
                                "    {:?}: Chip x{} @{}",
                                roll, amt, idx
                            ))),
                            strapped::Bet::Strap(s) => {
                                prev_lines.push(Line::from(format!(
                                    "    {:?}: {} x{} @{}",
                                    roll,
                                    render_reward_compact(s),
                                    amt,
                                    idx
                                )))
                            }
                        }
                    }
                }
            }
        }
    }
    let prev = Paragraph::new(prev_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Previous Games"),
    );
    f.render_widget(prev, lower[1]);
}

fn draw_bottom(f: &mut Frame, area: Rect, snap: &AppSnapshot) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(16), Constraint::Length(3)])
        .split(area);

    let status_widget = if snap.errors.is_empty() {
        let mut lines: Vec<Line> = Vec::new();
        if snap.status.trim().is_empty() {
            lines.push(Line::from("Ready"));
        } else {
            for line in snap.status.lines() {
                lines.push(Line::from(line.to_string()));
            }
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Status"))
            .style(Style::default().fg(Color::Green))
    } else {
        let mut lines: Vec<Line> = Vec::new();
        for e in &snap.errors {
            lines.push(Line::from(e.clone()));
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Errors"))
            .style(Style::default().fg(Color::Red))
    };
    f.render_widget(status_widget, chunks[0]);

    // Help
    let help = Paragraph::new(
        "←/→ select | b chip bet | t strap bet | B table bets | i straps | s shop | / VRF | m purchase | r roll | c claim | q/Esc quit",
    )
    .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, chunks[1]);
}

fn draw_wallet_panel(f: &mut Frame, area: Rect, snap: &AppSnapshot) {
    let (straps_line, has_more) = format_owned_strap_summary(&snap.owned_straps);

    let chips_balance = snap.chip_balance;
    let base_balance = snap.base_asset_balance;
    let asset_label = |asset_id: &AssetId, ticker: &Option<String>| {
        let asset_hex = hex::encode::<[u8; 32]>((*asset_id).into());
        let prefix_len = asset_hex.len().min(4);
        let prefix = format!("0x{}...", &asset_hex[..prefix_len]);
        match ticker.as_deref() {
            Some(ticker) => format!("{ticker} | {prefix}"),
            None => prefix,
        }
    };
    let chip_label = asset_label(&snap.chip_asset_id, &snap.chip_asset_ticker);
    let base_label = asset_label(&snap.base_asset_id, &snap.base_asset_ticker);
    let base_display = format_units(base_balance, 9);

    let mut text = format!(
        "Balance ({base_label}): {base_display} | Chips ({chip_label}): {chips_balance} | Straps: {straps_line}"
    );
    if has_more {
        text.push_str("... see more (press i)");
    }
    let widget = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Wallet"));
    f.render_widget(widget, area);
}

fn draw_overview_panel(f: &mut Frame, area: Rect, snap: &AppSnapshot) {
    let next_roll_text = match snap.next_roll_height {
        Some(h) => h.to_string(),
        None => String::from("N/A"),
    };
    let mut lines = Vec::new();
    let vrf_line = match snap.vrf_mode {
        VrfMode::Fake => {
            let vrf_roll = vrf_to_roll(snap.vrf_number);
            format!(
                "Game: {} | Fake VRF: {} ({:?}) | Next Roll Height: {} | Current Block Height: {}",
                snap.current_game_id,
                snap.vrf_number,
                vrf_roll,
                next_roll_text.as_str(),
                snap.current_block_height
            )
        }
        VrfMode::Pseudo => format!(
            "Game: {} | Pseudo VRF Mode | Next Roll Height: {} | Current Block Height: {}",
            snap.current_game_id,
            next_roll_text.as_str(),
            snap.current_block_height
        ),
    };
    lines.push(Line::from(vrf_line));
    lines.push(Line::from(format!(
        "Pot: {} | Owed: {} | Chip Bets: {} | Remaining Capacity: {}",
        snap.pot_balance,
        snap.chips_owed,
        snap.total_chip_bets,
        snap.available_bet_capacity
    )));
    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Game"))
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);
}

fn format_units(amount: u64, decimals: u32) -> String {
    let factor = 10u128.saturating_pow(decimals);
    let whole = (amount as u128) / factor;
    let fractional = (amount as u128) % factor;
    if fractional == 0 {
        whole.to_string()
    } else {
        let trimmed = format!("{:0width$}", fractional, width = decimals as usize)
            .trim_end_matches('0')
            .to_string();
        format!("{whole}.{trimmed}")
    }
}

fn draw_modals(f: &mut Frame, state: &UiState, snap: &AppSnapshot) {
    match &state.mode {
        Mode::BetModal(bs) => {
            let area = centered_rect(40, 30, f.area());
            let block = Block::default().borders(Borders::ALL).title("Place Bet");
            let p = Paragraph::new(format!(
                "Amount: {}\nEnter=confirm Esc=cancel +/- or digits to edit",
                bs.amount
            ));
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(p, block.inner(area));
        }
        Mode::TableBetsModal(_) => {
            let area = centered_rect(70, 70, f.area());
            let title = format!("Bets for {:?}", snap.selected_roll);
            let block = Block::default().borders(Borders::ALL).title(title);
            let chip_label = snap.chip_asset_ticker.as_deref().unwrap_or("chips");
            let mut lines = Vec::new();
            if let Some(cell) = snap
                .cells
                .iter()
                .find(|cell| cell.roll == snap.selected_roll)
            {
                lines.push(Line::from("You:"));
                lines.extend(format_bet_detail_lines(
                    cell.chip_total,
                    &cell.straps,
                    chip_label,
                    "  ",
                ));
                lines.push(Line::from(""));
                lines.push(Line::from("Table:"));
                if cell.table_bets.is_empty() {
                    lines.push(Line::from("  none"));
                } else {
                    for entry in &cell.table_bets {
                        lines.push(Line::from(entry.identity.clone()));
                        lines.extend(format_bet_detail_lines(
                            entry.chip_total,
                            &entry.straps,
                            chip_label,
                            "  ",
                        ));
                        lines.push(Line::from(""));
                    }
                }
                lines.push(Line::from(""));
                lines.push(Line::from("Esc=close"));
            } else {
                lines.push(Line::from("No data for selected roll"));
                lines.push(Line::from("Esc=close"));
            }
            let p = Paragraph::new(lines);
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(p, block.inner(area));
        }
        Mode::ClaimModal(cs) => {
            let area = centered_rect(60, 60, f.area());
            let block = Block::default()
                .borders(Borders::ALL)
                .title("Claim Rewards");
            let mut lines = Vec::new();
            let claimable = claimable_games(&snap.previous_games);
            if claimable.is_empty() {
                lines.push(Line::from("No claimable games"));
                lines.push(Line::from("Esc=close"));
            } else {
                // List all games with claimed status
                lines.push(Line::from("Games:"));
                for (i, g) in claimable.iter().enumerate() {
                    let cur = if i == cs.game_idx { ">" } else { " " };
                    lines.push(Line::from(format!(
                        "{} Game {} {}",
                        cur,
                        g.game_id,
                        game_status_label(g)
                    )));
                }
                // Details for selected game
                let idx = cs.game_idx.min(claimable.len() - 1);
                let g = &claimable[idx];
                lines.push(Line::from(""));
                lines.push(Line::from(format!("Details for Game {}", g.game_id)));
                // Rolls hit with modifier markers
                if g.rolls.is_empty() {
                    lines.push(Line::from("Rolls: None"));
                } else {
                    let mut items: Vec<String> = Vec::new();
                    for (i, r) in g.rolls.iter().enumerate() {
                        let mut emo = String::new();
                        for (mr, mm, mi) in &g.modifiers {
                            if mr == r && (*mi as usize) <= i {
                                let e = modifier_emoji(mm);
                                if !e.is_empty() {
                                    emo.push_str(e);
                                }
                            }
                        }
                        items.push(if emo.is_empty() {
                            format!("{:?}", r)
                        } else {
                            format!("{:?}{}", r, emo)
                        });
                    }
                    lines.push(Line::from(format!("Rolls: {}", items.join(" "))));
                }
                lines.push(Line::from("Bets:"));
                for (roll, bets) in &g.bets_by_roll {
                    for (bet, amt, idx) in bets {
                        match bet {
                            strapped::Bet::Chip => lines.push(Line::from(format!(
                                "  {:?}: Chip x{} @{}",
                                roll, amt, idx
                            ))),
                            strapped::Bet::Strap(s) => lines.push(Line::from(format!(
                                "  {:?}: {} x{} @{}",
                                roll,
                                render_reward_compact(s),
                                amt,
                                idx
                            ))),
                        }
                    }
                }
                lines.push(Line::from("Modifiers (space to toggle):"));
                for (i, (r, m, _idx)) in g.modifiers.iter().enumerate() {
                    let sel = cs.selected.iter().any(|(rr, mm)| rr == r && mm == m);
                    let cur = if i == cs.mod_idx { ">" } else { " " };
                    lines.push(Line::from(format!(
                        "{} [{}] {:?} {:?}",
                        cur,
                        if sel { "x" } else { " " },
                        r,
                        m
                    )));
                }
                let focus_hint = match cs.focus {
                    ClaimFocus::Games => "(focus: games)",
                    ClaimFocus::Modifiers => "(focus: modifiers)",
                };
                lines.push(Line::from(format!(
                    "Enter=claim Esc=cancel ←/→ focus ↑/↓ move Space toggle {}",
                    focus_hint
                )));
            }
            let p = Paragraph::new(lines);
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(p, block.inner(area));
        }
        Mode::VrfModal(vs) => {
            let area = centered_rect(50, 30, f.area());
            let block = Block::default()
                .borders(Borders::ALL)
                .title("Set VRF Number");
            let p = Paragraph::new(format!(
                "VRF: {}\nEnter=confirm Esc=cancel +/- or digits to edit",
                vs.value
            ));
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(p, block.inner(area));
        }
        Mode::ShopModal(ss) => {
            let area = centered_rect(60, 60, f.area());
            let block = Block::default()
                .borders(Borders::ALL)
                .title("Modifier Shop");
            let mut lines = Vec::new();
            if state.shop_items.is_empty() {
                lines.push(Line::from("No modifiers available"));
            } else {
                for (i, (from, to, modifier, triggered, purchased, price)) in
                    state.shop_items.iter().enumerate()
                {
                    let cur = if i == ss.idx { ">" } else { " " };
                    let text = if *purchased {
                        format!(
                            "{} {:?} {} - purchased ({price} chips)",
                            cur,
                            to,
                            modifier_emoji(modifier)
                        )
                    } else if *triggered {
                        format!(
                            "{} {:?} {} - {price} chips",
                            cur,
                            to,
                            modifier_emoji(modifier)
                        )
                    } else {
                        format!(
                            "{} {:?} {} (Unlock by rolling {:?}) - {price} chips",
                            cur,
                            to,
                            modifier_emoji(modifier),
                            from,
                        )
                    };
                    let line = if *purchased {
                        Line::styled(text, Style::default().fg(Color::Green))
                    } else if *triggered {
                        Line::from(text)
                    } else {
                        Line::styled(text, Style::default().fg(Color::DarkGray))
                    };
                    lines.push(line);
                }
                lines.push(Line::from(
                    "Enter=buy Esc=close ↑/↓ move (unlock first; purchased items are disabled)",
                ));
            }
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(Paragraph::new(lines), block.inner(area));
        }
        Mode::StrapBet(sb) => {
            let area = centered_rect(60, 50, f.area());
            let block = Block::default()
                .borders(Borders::ALL)
                .title("Place Strap Bet");
            let mut lines = Vec::new();
            if state.owned_straps.is_empty() {
                lines.push(Line::from("No straps owned"));
            } else {
                for (i, (s, bal)) in state.owned_straps.iter().enumerate() {
                    let cur = if i == sb.idx { ">" } else { " " };
                    lines.push(Line::from(format!(
                        "{} {} x{}",
                        cur,
                        render_reward_compact(s),
                        bal
                    )));
                }
                lines.push(Line::from(format!(
                    "Amount: {} (Enter=confirm, Esc=cancel, +/- change)",
                    sb.amount
                )));
            }
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(Paragraph::new(lines), block.inner(area));
        }
        Mode::StrapInventory(si) => {
            let area = centered_rect(70, 70, f.area());
            let block = Block::default()
                .borders(Borders::ALL)
                .title("Strap Inventory");
            let rows = aggregate_owned_straps_by_kind(&snap.owned_straps);
            let max_idx = rows.len().saturating_sub(1);
            let selected_idx = si.idx.min(max_idx);
            let mut lines = Vec::new();
            if rows.is_empty() {
                lines.push(Line::from("No strap types available"));
            } else {
                for (i, row) in rows.iter().enumerate() {
                    let prefix = if i == selected_idx { ">" } else { " " };
                    let kind_label = format!("{:?}", row.kind);
                    let detail = if row.entries.is_empty() {
                        String::from("-")
                    } else {
                        row.entries
                            .iter()
                            .map(|(strap, amount)| {
                                format!("{}x{}", render_reward_compact(strap), amount)
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    let text = format!(
                        "{} {:<10} total: {:<3} {}",
                        prefix, kind_label, row.total, detail
                    );
                    let line = if i == selected_idx {
                        Line::styled(
                            text,
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        )
                    } else {
                        Line::from(text)
                    };
                    lines.push(line);
                }
                let selected_kind = rows[selected_idx].kind.clone();
                let mut assets: Vec<(strapped::Strap, AssetId)> = snap
                    .known_straps
                    .iter()
                    .filter(|(_, strap)| strap.kind == selected_kind)
                    .map(|(asset_id, strap)| (strap.clone(), *asset_id))
                    .collect();
                assets.sort_by(|(a, _), (b, _)| {
                    a.level.cmp(&b.level).then_with(|| {
                        modifier_order_value(&a.modifier)
                            .cmp(&modifier_order_value(&b.modifier))
                    })
                });
                lines.push(Line::from(""));
                lines.push(Line::from(format!("Asset IDs for {:?}:", selected_kind)));
                if assets.is_empty() {
                    lines.push(Line::from("  None"));
                } else {
                    for (strap, asset_id) in assets {
                        lines.push(Line::from(format!(
                            "  {}: {}",
                            strap_asset_label(&strap),
                            asset_id_hex(&asset_id)
                        )));
                    }
                }
                lines.push(Line::from(""));
                lines.push(Line::from(
                    "Esc=close ↑/↓ move (rows include strap kinds you do not own yet)",
                ));
            }
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(Paragraph::new(lines), block.inner(area));
        }
        Mode::QuitModal => {
            let area = centered_rect(40, 20, f.area());
            let block = Block::default().borders(Borders::ALL).title("Confirm Quit");
            let p = Paragraph::new("Quit the game? (Y/N)");
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(p, block.inner(area));
        }
        Mode::Normal => {}
    }
}

#[derive(Clone, Debug, Default)]
struct VrfState {
    value: u64,
}

#[derive(Clone, Debug, Default)]
struct ShopState {
    idx: usize,
}

#[derive(Clone, Debug)]
struct StrapBetState {
    idx: usize,
    amount: u64,
}
impl Default for StrapBetState {
    fn default() -> Self {
        StrapBetState { idx: 0, amount: 1 }
    }
}

#[derive(Clone, Debug, Default)]
struct StrapInventoryState {
    idx: usize,
}

fn centered_rect(w_percent: u16, h_percent: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - h_percent) / 2),
            Constraint::Percentage(h_percent),
            Constraint::Percentage((100 - h_percent) / 2),
        ])
        .split(r);

    let vertical = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - w_percent) / 2),
            Constraint::Percentage(w_percent),
            Constraint::Percentage((100 - w_percent) / 2),
        ])
        .split(popup_layout[1]);

    vertical[1]
}

fn active_mods_emojis(
    roll: &strapped::Roll,
    active: &[(strapped::Roll, strapped::Modifier, u32)],
) -> String {
    let mut s = String::new();
    for (r, m, _) in active {
        if r == roll {
            let e = modifier_emoji(m);
            if !e.is_empty() {
                if !s.is_empty() {
                    s.push(' ');
                }
                s.push_str(e);
            }
        }
    }
    s
}

fn roll_border_style(
    roll: &strapped::Roll,
    selected: bool,
    active: &[(strapped::Roll, strapped::Modifier, u32)],
) -> Style {
    let mut style = Style::default();
    if let Some(color) = border_color_for_roll(roll, active) {
        style = style.fg(color);
    }
    if selected {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

fn border_color_for_roll(
    roll: &strapped::Roll,
    active: &[(strapped::Roll, strapped::Modifier, u32)],
) -> Option<Color> {
    active
        .iter()
        .find(|(r, m, _)| r == roll && *m != strapped::Modifier::Nothing)
        .map(|(_, m, _)| modifier_border_color(m))
}

fn modifier_border_color(m: &strapped::Modifier) -> Color {
    match m {
        strapped::Modifier::Nothing => Color::Rgb(108, 117, 125),
        strapped::Modifier::Burnt => Color::Rgb(220, 53, 69),
        strapped::Modifier::Lucky => Color::Rgb(40, 167, 69),
        strapped::Modifier::Holy => Color::Rgb(255, 193, 7),
        strapped::Modifier::Holey => Color::Rgb(108, 117, 125),
        strapped::Modifier::Scotch => Color::Rgb(139, 87, 42),
        strapped::Modifier::Soaked => Color::Rgb(0, 123, 255),
        strapped::Modifier::Moldy => Color::Rgb(111, 66, 193),
        strapped::Modifier::Starched => Color::Rgb(222, 226, 230),
        strapped::Modifier::Evil => Color::Rgb(156, 39, 176),
        strapped::Modifier::Groovy => Color::Rgb(255, 87, 34),
        strapped::Modifier::Delicate => Color::Rgb(255, 182, 193),
    }
}

fn strap_emoji(kind: &strapped::StrapKind) -> &'static str {
    match kind {
        strapped::StrapKind::Shirt => "👕",
        strapped::StrapKind::Pants => "👖",
        strapped::StrapKind::Shoes => "👟",
        strapped::StrapKind::Dress => "👗",
        strapped::StrapKind::Hat => "🎩",
        strapped::StrapKind::Glasses => "👓",
        strapped::StrapKind::Watch => "⌚",
        strapped::StrapKind::Ring => "💍",
        strapped::StrapKind::Necklace => "📿",
        strapped::StrapKind::Earring => "🧷",
        strapped::StrapKind::Bracelet => "🧶",
        strapped::StrapKind::Tattoo => "🐉",
        strapped::StrapKind::Skirt => "👚",
        strapped::StrapKind::Piercing => "📌",
        strapped::StrapKind::Coat => "🧥",
        strapped::StrapKind::Scarf => "🧣",
        strapped::StrapKind::Gloves => "🧤",
        strapped::StrapKind::Gown => "👘",
        strapped::StrapKind::Belt => "🧵",
    }
}

fn modifier_emoji(m: &strapped::Modifier) -> &'static str {
    match m {
        strapped::Modifier::Nothing => "",
        strapped::Modifier::Burnt => "🧯",
        strapped::Modifier::Lucky => "🍀",
        strapped::Modifier::Holy => "👼",
        strapped::Modifier::Holey => "🫥",
        strapped::Modifier::Scotch => "🏴",
        strapped::Modifier::Soaked => "🌊",
        strapped::Modifier::Moldy => "🍄",
        strapped::Modifier::Starched => "🏳️",
        strapped::Modifier::Evil => "😈",
        strapped::Modifier::Groovy => "✌️",
        strapped::Modifier::Delicate => "❤️",
    }
}

// Very tight reward format to reduce truncation: [modifier][kind][level]
// e.g., "🍄👕1" or "👕1" if no modifier
fn render_reward_compact(s: &strapped::Strap) -> String {
    let mod_emoji = modifier_emoji(&s.modifier);
    let kind_emoji = strap_emoji(&s.kind);
    if s.modifier == strapped::Modifier::Nothing {
        format!("{}{}", kind_emoji, s.level)
    } else {
        format!("{}{}{}", mod_emoji, kind_emoji, s.level)
    }
}

fn format_my_bet_lines(
    chip_total: u64,
    straps: &[(strapped::Strap, u64)],
    chip_label: &str,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if chip_total > 0 {
        lines.push(Line::from(format!("{chip_total} {chip_label}")));
    }
    for (strap, amount) in straps {
        lines.push(Line::from(format!(
            "{} x{}",
            render_reward_compact(strap),
            amount
        )));
    }
    if lines.is_empty() {
        lines.push(Line::from("none"));
    }
    lines
}

fn format_bet_detail_lines(
    chip_total: u64,
    straps: &[(strapped::Strap, u64)],
    chip_label: &str,
    indent: &str,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if chip_total > 0 {
        lines.push(Line::from(format!("{indent}{chip_total} {chip_label}")));
    }
    for (strap, amount) in straps {
        lines.push(Line::from(format!(
            "{indent}{} x{}",
            render_reward_compact(strap),
            amount
        )));
    }
    if lines.is_empty() {
        lines.push(Line::from(format!("{indent}none")));
    }
    lines
}

fn format_table_bet_lines(entry: &OtherPlayerBets, _chip_label: &str) -> Vec<String> {
    let mut parts = Vec::new();
    if entry.chip_total > 0 {
        parts.push(entry.chip_total.to_string());
    }
    for (strap, _amount) in &entry.straps {
        parts.push(render_reward_compact(strap));
    }
    if parts.is_empty() {
        vec![String::from("none")]
    } else {
        vec![parts.join(", ")]
    }
}

fn strap_asset_label(s: &strapped::Strap) -> String {
    if s.modifier == strapped::Modifier::Nothing {
        format!("lvl{}", s.level)
    } else {
        let modifier = modifier_emoji(&s.modifier);
        if modifier.is_empty() {
            format!("lvl{}", s.level)
        } else {
            format!("lvl{} {}", s.level, modifier)
        }
    }
}

fn asset_id_hex(asset_id: &AssetId) -> String {
    format!("0x{}", hex::encode::<[u8; 32]>((*asset_id).into()))
}

#[derive(Clone, Debug)]
struct StrapKindRow {
    kind: strapped::StrapKind,
    total: u64,
    entries: Vec<(strapped::Strap, u64)>,
}

const ALL_STRAP_KINDS: [strapped::StrapKind; 19] = [
    strapped::StrapKind::Shirt,
    strapped::StrapKind::Pants,
    strapped::StrapKind::Shoes,
    strapped::StrapKind::Dress,
    strapped::StrapKind::Hat,
    strapped::StrapKind::Glasses,
    strapped::StrapKind::Watch,
    strapped::StrapKind::Ring,
    strapped::StrapKind::Necklace,
    strapped::StrapKind::Earring,
    strapped::StrapKind::Bracelet,
    strapped::StrapKind::Tattoo,
    strapped::StrapKind::Skirt,
    strapped::StrapKind::Piercing,
    strapped::StrapKind::Coat,
    strapped::StrapKind::Scarf,
    strapped::StrapKind::Gloves,
    strapped::StrapKind::Gown,
    strapped::StrapKind::Belt,
];

fn strap_kind_catalog() -> &'static [strapped::StrapKind] {
    &ALL_STRAP_KINDS
}

fn aggregate_owned_straps_by_kind(owned: &[(strapped::Strap, u64)]) -> Vec<StrapKindRow> {
    let mut rows = Vec::new();
    for kind in strap_kind_catalog() {
        let mut entries: Vec<(strapped::Strap, u64)> = Vec::new();
        for (strap, amount) in owned.iter().filter(|(strap, _)| strap.kind == *kind) {
            if let Some((_, total)) =
                entries.iter_mut().find(|(existing, _)| existing == strap)
            {
                *total = total.saturating_add(*amount);
            } else {
                entries.push((strap.clone(), *amount));
            }
        }
        entries.sort_by(|(a, _), (b, _)| {
            a.level.cmp(&b.level).then_with(|| {
                modifier_order_value(&a.modifier).cmp(&modifier_order_value(&b.modifier))
            })
        });
        let total = entries.iter().map(|(_, amount)| *amount).sum();
        rows.push(StrapKindRow {
            kind: kind.clone(),
            total,
            entries,
        });
    }
    rows
}

fn format_owned_strap_summary(owned: &[(strapped::Strap, u64)]) -> (String, bool) {
    if owned.is_empty() {
        return (String::from("none"), false);
    }

    let mut aggregated: Vec<(strapped::Strap, u64)> = Vec::new();
    for (strap, amount) in owned {
        if let Some((_, total)) = aggregated
            .iter_mut()
            .find(|(existing, _)| existing == strap)
        {
            *total = total.saturating_add(*amount);
        } else {
            aggregated.push((strap.clone(), *amount));
        }
    }

    aggregated.sort_by(|(a, _), (b, _)| {
        a.level
            .cmp(&b.level)
            .then_with(|| {
                strap_kind_order_value(&a.kind).cmp(&strap_kind_order_value(&b.kind))
            })
            .then_with(|| {
                modifier_order_value(&a.modifier).cmp(&modifier_order_value(&b.modifier))
            })
    });

    let parts: Vec<String> = aggregated
        .into_iter()
        .map(|(strap, amount)| format!("{}x{}", render_reward_compact(&strap), amount))
        .collect();

    const MAX_DISPLAY: usize = 3;
    let has_more = parts.len() > MAX_DISPLAY;
    let displayed = if has_more {
        parts.into_iter().take(MAX_DISPLAY).collect::<Vec<_>>()
    } else {
        parts
    };

    (displayed.join(", "), has_more)
}

fn strap_kind_order_value(kind: &strapped::StrapKind) -> u8 {
    match kind {
        strapped::StrapKind::Shirt => 0,
        strapped::StrapKind::Pants => 1,
        strapped::StrapKind::Shoes => 2,
        strapped::StrapKind::Dress => 3,
        strapped::StrapKind::Hat => 4,
        strapped::StrapKind::Glasses => 5,
        strapped::StrapKind::Watch => 6,
        strapped::StrapKind::Ring => 7,
        strapped::StrapKind::Necklace => 8,
        strapped::StrapKind::Earring => 9,
        strapped::StrapKind::Bracelet => 10,
        strapped::StrapKind::Tattoo => 11,
        strapped::StrapKind::Skirt => 12,
        strapped::StrapKind::Piercing => 13,
        strapped::StrapKind::Coat => 14,
        strapped::StrapKind::Scarf => 15,
        strapped::StrapKind::Gloves => 16,
        strapped::StrapKind::Gown => 17,
        strapped::StrapKind::Belt => 18,
    }
}

fn modifier_order_value(modifier: &strapped::Modifier) -> u8 {
    match modifier {
        strapped::Modifier::Nothing => 0,
        strapped::Modifier::Burnt => 1,
        strapped::Modifier::Lucky => 2,
        strapped::Modifier::Holy => 3,
        strapped::Modifier::Holey => 4,
        strapped::Modifier::Scotch => 5,
        strapped::Modifier::Soaked => 6,
        strapped::Modifier::Moldy => 7,
        strapped::Modifier::Starched => 8,
        strapped::Modifier::Evil => 9,
        strapped::Modifier::Groovy => 10,
        strapped::Modifier::Delicate => 11,
    }
}

// Mirror the contract's VRF-to-roll mapping (2d6 distribution)
fn vrf_to_roll(num: u64) -> strapped::Roll {
    let modulo = num % 36;
    if modulo == 0 {
        strapped::Roll::Two
    } else if modulo <= 2 {
        strapped::Roll::Three
    } else if modulo <= 5 {
        strapped::Roll::Four
    } else if modulo <= 9 {
        strapped::Roll::Five
    } else if modulo <= 14 {
        strapped::Roll::Six
    } else if modulo <= 20 {
        strapped::Roll::Seven
    } else if modulo <= 25 {
        strapped::Roll::Eight
    } else if modulo <= 29 {
        strapped::Roll::Nine
    } else if modulo <= 32 {
        strapped::Roll::Ten
    } else if modulo <= 34 {
        strapped::Roll::Eleven
    } else {
        strapped::Roll::Twelve
    }
}
