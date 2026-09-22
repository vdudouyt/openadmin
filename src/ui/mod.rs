//! Top-level frame composition.
//!
//! The chrome always draws; modals composite on top with `Clear`. Render
//! functions take `&mut App` so they can write hitboxes back into
//! `app.regions` — hit-testing is a byproduct of layout, never a second
//! hand-maintained table.

pub mod bash;
pub mod dialogs;
pub mod header;
pub mod screens;
pub mod status_fn;
pub mod theme;
pub mod widgets;

use crate::app::{App, Mode, Screen};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::widgets::Block;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::new().style(theme::base()), area);
    app.regions.clear();

    // A terminal too small for the full chrome still gets the header, because
    // it is the only clickable way between screens — and on Shells the only
    // way out of a focused pane. Below three rows even that has to go.
    if area.height < 6 || area.width < 30 {
        let body = if area.height >= 3 {
            let [header_a, body_a] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
            header::render(f, header_a, app);
            body_a
        } else {
            area
        };
        match app.screen {
            Screen::Hosts => screens::hosts::render(f, body, app),
            Screen::Shells => screens::shells::render(f, body, app),
            Screen::Chat => screens::chat::render(f, body, app),
        }
        return;
    }

    // The Shells screen keeps no chrome of its own below the body: every key
    // there belongs to the terminal, so neither a function bar nor a status
    // line would earn the row it costs. Its tabs live on the header row, so
    // the whole screen spends a single row on itself.
    let chrome = if app.screen == Screen::Shells { 0 } else { 1 };
    let [header_a, body_a, status_a, fn_a] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(chrome),
        Constraint::Length(chrome),
    ])
    .areas(area);

    header::render(f, header_a, app);
    match app.screen {
        Screen::Hosts => screens::hosts::render(f, body_a, app),
        Screen::Shells => screens::shells::render(f, body_a, app),
        Screen::Chat => screens::chat::render(f, body_a, app),
    }
    if chrome > 0 {
        status_fn::render_status(f, status_a, app);
        status_fn::render_function_bar(f, fn_a, app);
    }

    match app.mode {
        Mode::HostForm => dialogs::host_form(f, app),
        Mode::ConfirmDelete => dialogs::confirm_delete(f, app),
        Mode::ConfirmPlan => dialogs::confirm_plan(f, app),
        Mode::ShowKey => dialogs::show_key(f, app),
        Mode::Help => dialogs::help(f, app),
        Mode::Mounting => dialogs::mounting(f, app),
        Mode::BulkImport => dialogs::bulk_import(f, app),
        Mode::Normal => {}
    }

    // An error sits on top of everything, including another dialog.
    if app.alert.is_some() {
        dialogs::alert(f, app);
    }
}
