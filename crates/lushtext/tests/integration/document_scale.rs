// SPDX-License-Identifier: GPL-3.0-or-later

//! The runtime Markdown chunked cap's test-only override.
//!
//! `markdown_chunked_cap` is what every runtime caller of the `_with` scale
//! functions passes. Only the widget tests drove it before, and the mutation
//! lane does not run them, so its body could be replaced without any test in
//! the lane failing. This binary always builds `lushtext-core` with
//! `test-utils`, the only build in which the override exists.
//!
//! The override is process-wide, so this is the binary's only test that sets
//! it. No other test here reads the cap: its callers are the GTK editor
//! adapters, which only the widget harness builds.

use lushtext_core::model::document_scale::{
    MODEL_MARKDOWN_CAP, markdown_chunked_cap, override_markdown_chunked_cap_for_test,
};

#[test]
fn the_runtime_markdown_cap_is_the_model_cap_only_while_the_override_is_held() {
    assert_eq!(markdown_chunked_cap(), None, "shipped: no chunked cap");
    {
        let _cap = override_markdown_chunked_cap_for_test();
        assert_eq!(markdown_chunked_cap(), Some(MODEL_MARKDOWN_CAP));
    }
    assert_eq!(markdown_chunked_cap(), None, "the guard removed the cap");
}
