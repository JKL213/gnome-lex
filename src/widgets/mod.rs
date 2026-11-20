// SPDX-FileCopyrightText: 2026 Gnome Lex
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Eigene Widgets: Gliederungsbaum, Normansicht, Leseansicht, Sprungleiste
//! sowie Karteikarte, Kartenstapel, Karteikasten und Lernansicht.

pub mod card_stack;
pub mod download_center;
pub mod flashcards;
pub mod hint_gutter;
pub mod index_card;
pub mod jump_bar;
pub mod leitner_box;
pub mod norm_view;
pub mod outline;
pub mod outline_row;
pub mod preferences;
pub mod reader;

pub use card_stack::LexCardStack;
pub use download_center::LexDownloadCenter;
pub use flashcards::LexFlashcards;
pub use index_card::LexIndexCard;
pub use jump_bar::LexJumpBar;
pub use leitner_box::LexLeitnerBox;
pub use norm_view::LexNormView;
pub use outline::Outline;
pub use outline_row::LexOutlineRow;
pub use preferences::LexPreferences;
pub use reader::{LexReader, ReaderState, TabsState};
