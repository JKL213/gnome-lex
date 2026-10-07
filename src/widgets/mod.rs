// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Eigene Widgets: Gliederungsbaum, Normansicht, Tabs und Schnellsuche.

pub mod download_center;
pub mod law_tab;
pub mod norm_view;
pub mod outline;
pub mod outline_row;
pub mod preferences;
pub mod quick_search;

pub use download_center::LexDownloadCenter;
pub use law_tab::{LexLawTab, TabState};
pub use norm_view::LexNormView;
pub use outline::Outline;
pub use outline_row::LexOutlineRow;
pub use preferences::LexPreferences;
pub use quick_search::LexQuickSearch;
