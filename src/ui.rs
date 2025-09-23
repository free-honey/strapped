use crate::client::{all_rolls, AppSnapshot, PreviousGameSummary, RollCell};
use strapped_contract::strapped_types as strapped;
use color_eyre::eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::io::stdout;

pub enum UserEvent {
    Quit,
    NextRoll,
    PrevRoll,
    Owner,
    Alice,
    PlaceBet,
    PlaceBetAmount(u64),
    Purchase,
    Roll,
    VRFInc,
    VRFDec,
    SetVrf(u64),
    Claim,
    OpenBetModal,
    OpenClaimModal,
    OpenVrfModal,
    Redraw,
    OpenShop,
    ConfirmShopPurchase { roll: strapped::Roll, modifier: strapped::Modifier },
    OpenStrapBet,
    ConfirmStrapBet { strap: strapped::Strap, amount: u64 },
    ConfirmClaim { game_id: u64, enabled: Vec<(strapped::Roll, strapped::Modifier)> },
}

#[derive(Debug)]
pub struct UiState {
    mode: Mode,
    prev_games: Vec<PreviousGameSummary>,
    current_vrf: u64,
    terminal: Option<Terminal<CrosstermBackend<std::io::Stdout>>>,
    shop_items: Vec<(strapped::Roll, strapped::Roll, strapped::Modifier, bool)>,
    last_game_id: Option<u64>,
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
    ClaimModal(ClaimState),
    VrfModal(VrfState),
    ShopModal(ShopState),
    QuitModal,
    StrapBet(StrapBetState),
}

#[derive(Clone, Debug)]
struct BetState { amount: u64 }

impl Default for BetState { fn default() -> Self { BetState { amount: 0 } } }

#[derive(Clone, Debug)]
struct ClaimState {
    game_idx: usize,
    mod_idx: usize,
    selected: Vec<(strapped::Roll, strapped::Modifier)>,
}

impl Default for ClaimState { fn default() -> Self { ClaimState { game_idx: 0, mod_idx: 0, selected: Vec::new() } } }

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
    let game_changed = state.last_game_id.map_or(true, |g| g != snap.current_game_id);
    state.shop_items = snap.modifier_triggers.clone();
    state.owned_straps = snap.owned_straps.clone();
    if game_changed {
        // reset selection index if currently in shop
        if let Mode::ShopModal(ref mut ss) = state.mode { ss.idx = 0; }
        state.last_game_id = Some(snap.current_game_id);
    }
    if let Some(mut term) = state.terminal.take() {
        term.draw(|f| ui(f, state, snap))?;
        state.terminal = Some(term);
    }
    Ok(())
}

pub async fn next_event(state: &mut UiState) -> Result<UserEvent> {
    loop {
        if let Event::Key(k) = event::read()? {
            if k.kind != KeyEventKind::Press { continue; }
            // Modal handling
            match &mut state.mode {
                Mode::BetModal(bs) => {
                    match k.code {
                        KeyCode::Esc => { state.mode = Mode::Normal; return Ok(UserEvent::Redraw); }
                        KeyCode::Enter => { let amt = bs.amount; state.mode = Mode::Normal; return Ok(UserEvent::PlaceBetAmount(amt)); }
                        KeyCode::Up | KeyCode::Char('+') => { bs.amount = bs.amount.saturating_add(1); return Ok(UserEvent::Redraw); }
                        KeyCode::Down | KeyCode::Char('-') => { bs.amount = bs.amount.saturating_sub(1); return Ok(UserEvent::Redraw); }
                        KeyCode::Backspace => { bs.amount /= 10; return Ok(UserEvent::Redraw); }
                        KeyCode::Char(c) if c.is_ascii_digit() => { let d = c.to_digit(10).unwrap() as u64; bs.amount = (bs.amount.saturating_mul(10)).saturating_add(d); return Ok(UserEvent::Redraw); }
                        _ => {}
                    }
                }
                Mode::ClaimModal(cs) => {
                    match k.code {
                        KeyCode::Esc => { state.mode = Mode::Normal; return Ok(UserEvent::Redraw); }
                        KeyCode::Left => { if cs.game_idx>0 { cs.game_idx-=1; } return Ok(UserEvent::Redraw); }
                        KeyCode::Right => { let len = state.prev_games.len(); if len>0 { cs.game_idx=(cs.game_idx+1).min(len-1);} return Ok(UserEvent::Redraw); }
                        KeyCode::Up => { if cs.mod_idx>0 { cs.mod_idx-=1; } return Ok(UserEvent::Redraw); }
                        KeyCode::Down => { if let Some(g)=state.prev_games.get(cs.game_idx){ if !g.modifiers.is_empty(){ cs.mod_idx=(cs.mod_idx+1).min(g.modifiers.len()-1);} } return Ok(UserEvent::Redraw); }
                        KeyCode::Char(' ') => {
                            if let Some(g)=state.prev_games.get(cs.game_idx){ if let Some((r,m,_idx))=g.modifiers.get(cs.mod_idx){ if let Some(pos)=cs.selected.iter().position(|(rr,mm)| rr==r && mm==m){ cs.selected.remove(pos);} else { cs.selected.push((r.clone(), m.clone())); } } }
                            return Ok(UserEvent::Redraw);
                        }
                        KeyCode::Enter => {
                            if let Some(g)=state.prev_games.get(cs.game_idx){ let enabled=cs.selected.clone(); let game_id=g.game_id; state.mode=Mode::Normal; return Ok(UserEvent::ConfirmClaim{ game_id, enabled }); }
                            continue;
                        }
                        _ => {}
                    }
                }
                Mode::VrfModal(vs) => {
                    match k.code {
                        KeyCode::Esc => { state.mode = Mode::Normal; return Ok(UserEvent::Redraw); }
                        KeyCode::Up | KeyCode::Char('+') => { vs.value = vs.value.saturating_add(1); return Ok(UserEvent::Redraw); }
                        KeyCode::Down | KeyCode::Char('-') => { vs.value = vs.value.saturating_sub(1); return Ok(UserEvent::Redraw); }
                        KeyCode::Backspace => { vs.value /= 10; return Ok(UserEvent::Redraw); }
                        KeyCode::Char(c) if c.is_ascii_digit() => { let d = c.to_digit(10).unwrap() as u64; vs.value = vs.value.saturating_mul(10).saturating_add(d); return Ok(UserEvent::Redraw); }
                        KeyCode::Enter => { let n = vs.value; state.mode = Mode::Normal; return Ok(UserEvent::SetVrf(n)); }
                        _ => {}
                    }
                }
                Mode::ShopModal(ss) => {
                    match k.code {
                        KeyCode::Esc => { state.mode = Mode::Normal; return Ok(UserEvent::Redraw); }
                        KeyCode::Up => { if ss.idx>0 { ss.idx-=1; } return Ok(UserEvent::Redraw); }
                        KeyCode::Down => { let max = state.shop_items.len().saturating_sub(1); ss.idx = (ss.idx+1).min(max); return Ok(UserEvent::Redraw); }
                        KeyCode::Enter => {
                            if let Some((from, to, m, on)) = state.shop_items.get(ss.idx).cloned() {
                                if on {
                                    state.mode = Mode::Normal;
                                    return Ok(UserEvent::ConfirmShopPurchase { roll: to, modifier: m });
                                } else {
                                    return Ok(UserEvent::Redraw);
                                }
                            } else { return Ok(UserEvent::Redraw); }
                        }
                        _ => {}
                    }
                }
                Mode::StrapBet(sb) => {
                    match k.code {
                        KeyCode::Esc => { state.mode = Mode::Normal; return Ok(UserEvent::Redraw); }
                        KeyCode::Up => { if sb.idx>0 { sb.idx-=1; } return Ok(UserEvent::Redraw); }
                        KeyCode::Down => { let max = state.owned_straps.len().saturating_sub(1); sb.idx = (sb.idx+1).min(max); return Ok(UserEvent::Redraw); }
                        KeyCode::Char('+') | KeyCode::Right => { sb.amount = sb.amount.saturating_add(1); return Ok(UserEvent::Redraw); }
                        KeyCode::Char('-') | KeyCode::Left => { sb.amount = sb.amount.saturating_sub(1).max(1); return Ok(UserEvent::Redraw); }
                        KeyCode::Enter => {
                            if let Some((s, bal)) = state.owned_straps.get(sb.idx).cloned() {
                                let amt = sb.amount.min(bal);
                                state.mode = Mode::Normal;
                                return Ok(UserEvent::ConfirmStrapBet { strap: s, amount: amt });
                            } else { return Ok(UserEvent::Redraw); }
                        }
                        _ => {}
                    }
                }
                Mode::QuitModal => {
                    match k.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => { return Ok(UserEvent::Quit); }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => { state.mode = Mode::Normal; return Ok(UserEvent::Redraw); }
                        _ => {}
                    }
                }
                Mode::Normal => {}
            }
            return Ok(match k.code {
                KeyCode::Char('q') | KeyCode::Esc => { state.mode = Mode::QuitModal; UserEvent::Redraw },
                KeyCode::Right => UserEvent::NextRoll,
                KeyCode::Left => UserEvent::PrevRoll,
                KeyCode::Char('o') => UserEvent::Owner,
                KeyCode::Char('a') => UserEvent::Alice,
                KeyCode::Char('b') => { state.mode = Mode::BetModal(BetState::default()); UserEvent::OpenBetModal },
                KeyCode::Char('t') => { state.mode = Mode::StrapBet(StrapBetState::default()); UserEvent::OpenStrapBet },
                KeyCode::Char('m') => UserEvent::Purchase,
                KeyCode::Char('r') => UserEvent::Roll,
                KeyCode::Char(']') => UserEvent::VRFInc,
                KeyCode::Char('[') => UserEvent::VRFDec,
                KeyCode::Char('/') => { state.mode = Mode::VrfModal(VrfState::default()); UserEvent::OpenVrfModal },
                KeyCode::Char('s') => { state.mode = Mode::ShopModal(ShopState::default()); UserEvent::OpenShop },
                KeyCode::Char('c') => { state.mode = Mode::ClaimModal(ClaimState::default()); UserEvent::OpenClaimModal },
                _ => continue,
            });
        }
    }
}

fn ui(f: &mut Frame, state: &UiState, snap: &AppSnapshot) {
    // Clear the whole frame to avoid leftover fragments
    f.render_widget(Clear, f.area());
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // status
            Constraint::Length(3),  // roll history
            Constraint::Length(17), // horizontal grid (even taller cells)
            Constraint::Length(16), // shop + previous games (about 4x taller)
            Constraint::Length(6),  // errors + help
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
    let wallet = match snap.wallet { crate::client::WalletKind::Owner => "Owner", _ => "Alice" };
    let vrf_roll = vrf_to_roll(snap.vrf_number);
    // Build compact strap list
    let mut strap_items: Vec<String> = Vec::new();
    for (s, bal) in &snap.owned_straps {
        strap_items.push(format!("{} x{}", render_reward_compact(s), bal));
    }
    let straps_line = if strap_items.is_empty() { String::from("none") } else { strap_items.join(" ") };
    let gauge = Paragraph::new(format!(
        "Wallet: {} | Chips: {} | Straps: {} | Pot: {} | Game: {} | VRF: {} ({:?})\n{}",
        wallet, snap.chip_balance, straps_line, snap.pot_balance, snap.current_game_id, snap.vrf_number, vrf_roll, snap.status
    ))
        .style(Style::default())
        .block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(gauge, area);
}

fn draw_grid(f: &mut Frame, area: Rect, snap: &AppSnapshot) {
    // Draw rolls 2..12 with 7 included
    let rolls = &snap.cells; // already ordered: [Two..Twelve], Seven in middle
    let cols = rolls.len() as u16; // 11
    let col_w = if cols > 0 { area.width / cols } else { area.width };
    for (i, cell) in rolls.iter().enumerate() {
        let c = i as u16;
        let rect = Rect::new(area.x + c * col_w, area.y, col_w, area.height);
        let selected = cell.roll == snap.selected_roll;
        let mut lines = vec![ Line::from(format!("Chips: {}", cell.chip_total)) ];
        lines.push(Line::from("Straps:"));
        for (strap, amt) in &cell.straps { lines.push(render_strap_line(strap, *amt)); }
        // Rewards list
        lines.push(Line::from("Rewards:"));
        if cell.rewards.is_empty() {
            lines.push(Line::from("  None"));
        } else {
            for (s, bal) in &cell.rewards {
                lines.push(Line::from(format!("  {} x{}", render_reward_compact(s), bal)));
            }
        }
        let label = Paragraph::new(lines);
        let mods = active_mods_emojis(&cell.roll, &snap.active_modifiers);
        let base = match cell.roll {
            strapped::Roll::Seven => String::from("Seven (RESET)"),
            _ => format!("{:?}", cell.roll),
        };
        let title = if mods.is_empty() { base } else { format!("{} {}", base, mods) };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                title,
                if selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() },
            ));
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
        let items: Vec<String> = snap.roll_history.iter().map(|r| format!("{:?}", r)).collect();
        rh.push(Line::from(items.join(" ")));
    }
    let roll_hist = Paragraph::new(rh).block(Block::default().borders(Borders::ALL).title("Roll History"));
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
        for (from, to, modifier, on) in &state.shop_items {
            let text = if *on {
                format!("{:?} {}", to, modifier_emoji(modifier))
            } else {
                format!("{:?} {} (Unlock by rolling {:?})", to, modifier_emoji(modifier), from)
            };
            if *on {
                shop_lines.push(Line::from(text));
            } else {
                shop_lines.push(Line::styled(text, Style::default().fg(Color::DarkGray)));
            }
        }
    }
    let shop = Paragraph::new(shop_lines).block(Block::default().borders(Borders::ALL).title("Shop"));
    f.render_widget(shop, lower[0]);

    // Previous games on the right (latest at top)
    let mut prev_lines = vec![];
    if snap.previous_games.is_empty() {
        prev_lines.push(Line::from("None"));
    } else {
        for g in &snap.previous_games {
            let claimed = if g.claimed { "[claimed]" } else { "[unclaimed]" };
            prev_lines.push(Line::from(format!("Game {} {}", g.game_id, claimed)));
            // Rolls line
            if g.rolls.is_empty() {
                prev_lines.push(Line::from("  Rolls: None"));
            } else {
                let mut items: Vec<String> = Vec::new();
                for (idx, r) in g.rolls.iter().enumerate() {
                    // Append emojis for modifiers active at this roll index
                    let mut emo = String::new();
                    for (mr, mm, mi) in &g.modifiers {
                        if mr == r && (*mi as usize) <= idx {
                            let e = modifier_emoji(mm);
                            if !e.is_empty() { emo.push_str(e); }
                        }
                    }
                    items.push(if emo.is_empty() { format!("{:?}", r) } else { format!("{:?}{}", r, emo) });
                }
                prev_lines.push(Line::from(format!("  Rolls: {}", items.join(" "))));
            }
            // Bets list with indices
            prev_lines.push(Line::from("  Bets:"));
            let mut any_bets = false;
            for (roll, bets) in &g.bets_by_roll {
                for (bet, amt, idx) in bets {
                    any_bets = true;
                    match bet {
                        strapped::Bet::Chip => prev_lines.push(Line::from(format!("    {:?}: Chip x{} @{}", roll, amt, idx))),
                        strapped::Bet::Strap(s) => prev_lines.push(Line::from(format!("    {:?}: {} x{} @{}", roll, render_reward_compact(s), amt, idx))),
                    }
                }
            }
            if !any_bets { prev_lines.push(Line::from("    None")); }
        }
    }
    let prev = Paragraph::new(prev_lines).block(Block::default().borders(Borders::ALL).title("Previous Games"));
    f.render_widget(prev, lower[1]);
}

fn draw_bottom(f: &mut Frame, area: Rect, snap: &AppSnapshot) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3)])
        .split(area);

    // Errors/logs
    let mut lines: Vec<Line> = Vec::new();
    if snap.errors.is_empty() {
        lines.push(Line::from("No errors"));
    } else {
        for e in &snap.errors { lines.push(Line::from(e.clone())); }
    }
    let errors = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Errors"));
    let color = if snap.roll_history.is_empty() && snap.previous_games.is_empty() {
        // No activity yet — keep neutral
        Color::DarkGray
    } else if snap.errors.is_empty() { Color::Green } else { Color::Red };
    f.render_widget(errors.style(Style::default().fg(color)), chunks[0]);

    // Help
    let help = Paragraph::new(
        "←/→ select | a Alice | o Owner | b chip bet | t strap bet | s shop | / VRF | m purchase | r roll | c claim | q/Esc quit",
    )
    .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, chunks[1]);
}

fn draw_modals(f: &mut Frame, state: &UiState, snap: &AppSnapshot) {
    match &state.mode {
        Mode::BetModal(bs) => {
            let area = centered_rect(40, 30, f.area());
            let block = Block::default().borders(Borders::ALL).title("Place Bet");
            let p = Paragraph::new(format!("Amount: {}\nEnter=confirm Esc=cancel +/- or digits to edit", bs.amount));
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(p, block.inner(area));
        }
        Mode::ClaimModal(cs) => {
            let area = centered_rect(60, 60, f.area());
            let block = Block::default().borders(Borders::ALL).title("Claim Rewards");
            let mut lines = Vec::new();
            if snap.previous_games.is_empty() {
                lines.push(Line::from("No previous games"));
            } else {
                // List all games with claimed status
                lines.push(Line::from("Games:"));
                for (i, g) in snap.previous_games.iter().enumerate() {
                    let cur = if i == cs.game_idx { ">" } else { " " };
                    let claimed = if g.claimed { "[claimed]" } else { "[unclaimed]" };
                    lines.push(Line::from(format!("{} Game {} {}", cur, g.game_id, claimed)));
                }
                // Details for selected game
                let idx = cs.game_idx.min(snap.previous_games.len()-1);
                let g = &snap.previous_games[idx];
                lines.push(Line::from(""));
                lines.push(Line::from(format!("Details for Game {}", g.game_id)));
                lines.push(Line::from("Bets:"));
                for cell in &g.cells { if cell.chip_total>0 || cell.strap_total>0 { lines.push(Line::from(format!("  {:?}: chip {} strap {}", cell.roll, cell.chip_total, cell.strap_total))); } }
                lines.push(Line::from("Modifiers (space to toggle):"));
                for (i,(r,m,_idx)) in g.modifiers.iter().enumerate() {
                    let sel = cs.selected.iter().any(|(rr,mm)| rr==r && mm==m);
                    let cur = if i==cs.mod_idx { ">" } else { " " };
                    lines.push(Line::from(format!("{} [{}] {:?} {:?}", cur, if sel {"x"} else {" "}, r, m)));
                }
                lines.push(Line::from("Enter=claim Esc=cancel ←/→ game ↑/↓ select"));
            }
            let p = Paragraph::new(lines);
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(p, block.inner(area));
        }
        Mode::VrfModal(vs) => {
            let area = centered_rect(50, 30, f.area());
            let block = Block::default().borders(Borders::ALL).title("Set VRF Number");
            let p = Paragraph::new(format!("VRF: {}\nEnter=confirm Esc=cancel +/- or digits to edit", vs.value));
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(p, block.inner(area));
        }
        Mode::ShopModal(ss) => {
            let area = centered_rect(60, 60, f.area());
            let block = Block::default().borders(Borders::ALL).title("Modifier Shop (price: 1 chip)");
            let mut lines = Vec::new();
            if state.shop_items.is_empty() {
                lines.push(Line::from("No modifiers available"));
            } else {
                for (i, (from, to, modifier, on)) in state.shop_items.iter().enumerate() {
                    let cur = if i == ss.idx { ">" } else { " " };
                    let text = if *on {
                        format!("{} {:?} {}", cur, to, modifier_emoji(modifier))
                    } else {
                        format!("{} {:?} {} (Unlock by rolling {:?})", cur, to, modifier_emoji(modifier), from)
                    };
                    if *on {
                        lines.push(Line::from(text));
                    } else {
                        lines.push(Line::styled(text, Style::default().fg(Color::DarkGray)));
                    }
                }
                lines.push(Line::from("Enter=buy Esc=close ↑/↓ move"));
            }
            f.render_widget(Clear, area);
            f.render_widget(block.clone(), area);
            f.render_widget(Paragraph::new(lines), block.inner(area));
        }
        Mode::StrapBet(sb) => {
            let area = centered_rect(60, 50, f.area());
            let block = Block::default().borders(Borders::ALL).title("Place Strap Bet");
            let mut lines = Vec::new();
            if state.owned_straps.is_empty() {
                lines.push(Line::from("No straps owned"));
            } else {
                for (i, (s, bal)) in state.owned_straps.iter().enumerate() {
                    let cur = if i == sb.idx { ">" } else { " " };
                    lines.push(Line::from(format!("{} {} x{}", cur, render_reward_compact(s), bal)));
                }
                lines.push(Line::from(format!("Amount: {} (Enter=confirm, Esc=cancel, +/- change)", sb.amount)));
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

#[derive(Clone, Debug)]
struct VrfState { value: u64 }

impl Default for VrfState { fn default() -> Self { VrfState { value: 0 } } }

#[derive(Clone, Debug, Default)]
struct ShopState { idx: usize }
#[derive(Clone, Debug)]
struct StrapBetState { idx: usize, amount: u64 }
impl Default for StrapBetState { fn default() -> Self { StrapBetState { idx: 0, amount: 1 } } }

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

fn active_mods_emojis(roll: &strapped::Roll, active: &Vec<(strapped::Roll, strapped::Modifier, u64)>) -> String {
    let mut s = String::new();
    for (r, m, _) in active {
        if r == roll {
            let e = modifier_emoji(m);
            if !e.is_empty() {
                if !s.is_empty() { s.push(' '); }
                s.push_str(e);
            }
        }
    }
    s
}

fn strap_emoji(kind: &strapped::StrapKind) -> &'static str {
    match kind {
        strapped::StrapKind::Shirt => "👕",
        strapped::StrapKind::Pants => "👖",
        strapped::StrapKind::Shoes => "👟",
        strapped::StrapKind::Hat => "🎩",
        strapped::StrapKind::Glasses => "👓",
        strapped::StrapKind::Watch => "⌚",
        strapped::StrapKind::Ring => "💍",
        strapped::StrapKind::Necklace => "📿",
        strapped::StrapKind::Earring => "🧷",
        strapped::StrapKind::Bracelet => "🧶",
        strapped::StrapKind::Tattoo => "🎨",
        strapped::StrapKind::Piercing => "📌",
        strapped::StrapKind::Coat => "🧥",
        strapped::StrapKind::Scarf => "🧣",
        strapped::StrapKind::Gloves => "🧤",
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
    }
}

fn level_style(level: u8) -> Style {
    match level {
        1 => Style::default().fg(Color::White),
        2 => Style::default().fg(Color::Green),
        3 => Style::default().fg(Color::Yellow),
        4 => Style::default().fg(Color::Blue),
        5 => Style::default().fg(Color::Magenta),
        _ => Style::default().fg(Color::Cyan),
    }
}

fn render_strap_compact(s: &strapped::Strap) -> String {
    let mod_emoji = modifier_emoji(&s.modifier);
    let kind_emoji = strap_emoji(&s.kind);
    if s.modifier == strapped::Modifier::Nothing {
        format!("lvl{} {}", s.level, kind_emoji)
    } else {
        format!("lvl{} {} {}", s.level, mod_emoji, kind_emoji)
    }
}

fn render_strap_line(s: &strapped::Strap, amount: u64) -> Line<'static> {
    let text = render_strap_compact(s);
    Line::styled(format!("{} x{}", text, amount), level_style(s.level))
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
