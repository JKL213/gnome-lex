// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Eigene Widgets: Gliederungsbaum, Normansicht, Leseansicht und Sprungleiste.

pub mod download_center;
pub mod jump_bar;
pub mod norm_view;
pub mod outline;
pub mod outline_row;
pub mod preferences;
pub mod reader;

pub use download_center::LexDownloadCenter;
pub use jump_bar::LexJumpBar;
pub use norm_view::LexNormView;
pub use outline::Outline;
pub use outline_row::LexOutlineRow;
pub use preferences::LexPreferences;
pub use reader::{LexReader, ReaderState};
