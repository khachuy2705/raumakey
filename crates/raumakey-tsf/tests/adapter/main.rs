//! Tests of the TSF adapter rules (`raumakey_tip::adapter`): what the COM layer decides on
//! its own, apart from COM, run against fakes on any host (design §15.1). Passing them says
//! nothing about runtime behavior inside a Windows host application.

mod documents;
mod edit;
mod host;
mod keypath;
mod keys;
mod mode;
mod mouse;
mod record;
mod scope;
