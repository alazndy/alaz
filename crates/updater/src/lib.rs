//! Checks GitHub for a newer release of the game, downloads it, checks its SHA-256 and hands it to the platform's
//! installer. Nothing here runs by itself: the game starts a check on a background thread and installs only after the
//! player presses the button.

pub mod install;
pub mod net;
pub mod release;
pub mod verify;
pub mod version;
