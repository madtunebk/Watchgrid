//! Signing in. There is deliberately no sign-up, password reset or user
//! administration here: accounts are managed only from the server CLI.

mod gate;
mod login;

pub use gate::AuthGate;
