// SPDX-License-Identifier: GPL-3.0-or-later

//! LushText binary entry point.

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lushtext=info".into()),
        )
        .init();

    lushtext_core::run()
}
