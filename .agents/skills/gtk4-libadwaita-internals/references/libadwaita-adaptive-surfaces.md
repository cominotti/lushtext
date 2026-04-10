# Libadwaita Adaptive Surfaces

## Table of Contents

- `AdwBreakpoint`
- `AdwNavigationSplitView`
- `AdwToolbarView`
- `AdwViewStack`
- Rust implications

## `AdwBreakpoint`

`AdwBreakpoint` is the declarative mechanism for adaptive layout changes in `AdwWindow`, `AdwApplicationWindow`, `AdwDialog`, and `AdwBreakpointBin`.

Official docs define a breakpoint as:

- a condition
- one or more setters
- automatic application and rollback of the original property values

Important source-backed failure modes from `src/adw-breakpoint.c`:

- invalid condition strings produce `Unable to parse condition`
- setting an unknown property produces `Type '...' does not have a property named '...'`
- missing Builder object IDs produce `Unable to find object '...' for setter`
- invalid values produce `Invalid value ... for property ...`

This means breakpoint errors are configuration errors first. Fix the parsed condition, object ID, property name, or value type before reasoning about adaptive behavior.

## `AdwNavigationSplitView`

`AdwNavigationSplitView` has two `AdwNavigationPage` children:

- sidebar
- content

When uncollapsed:

- it measures sidebar and content separately
- it clamps sidebar width between explicit minimum and maximum sidebar widths
- it derives sidebar natural width from the content natural width and the sidebar fraction
- it allocates sidebar and content side by side

When collapsed:

- it places both pages into an `AdwNavigationView`
- `show-content` chooses which page is visible
- header bars integrate automatically, including back button and title behavior

Important source-backed invariants:

- sidebar and content tags must be unique
- wrong child types cause `Cannot add an object of type ... to AdwNavigationSplitView`
- trying to reuse an already-parented page is rejected
- pushing a tag that already represents the active content or sidebar page can trigger navigation-stack criticals

Treat this widget as both a layout container and a navigation container. Bugs can be geometric, but they can also be tag or page-identity bugs.

## `AdwToolbarView`

`AdwToolbarView` combines:

- a single content widget
- zero or more top bars
- zero or more bottom bars

Important documented behavior:

- bars can be typed as `top` or `bottom` in Builder XML
- content can extend behind top or bottom bars
- top and bottom bars can be revealed or hidden with animation
- undershoot style classes express edge behavior when content touches bars

Important source-backed layout behavior from `src/adw-toolbar-view.c`:

- top bar, bottom bar, and content are all measured
- content height is reduced by bar heights unless content is allowed to extend behind them
- actual bar heights are clamped from available height against their minimum and natural sizes
- the widget updates `top-bar-height` and `bottom-bar-height` properties from real allocation results

So if a toolbar view feels visually simple but causes geometry churn, debug it as a three-part measuring widget: top bars, content, and bottom bars all participate.

## `AdwViewStack`

`AdwViewStack` is conceptually simple but has a strong naming invariant:

- page names must be unique

Source warnings from `src/adw-view-stack.c` include:

- `While adding page: duplicate child name in AdwViewStack: ...`
- `Duplicate child name in AdwViewStack: ...`
- `Child name '...' not found in AdwViewStack`

That means page routing, visible-child changes, and associated switchers are name-sensitive. If stack navigation behaves inconsistently, inspect page names before touching transitions or selection widgets.

## Rust Implications

- Prefer declarative breakpoints over imperative resize logic when adapting Libadwaita layouts.
- Use `AdwNavigationPage` where `AdwNavigationSplitView` expects it. A plain widget is the wrong type even if it looks visually correct.
- Keep page tags and view-stack page names unique. These are routing identifiers, not labels.
- Remember that Libadwaita widgets are still GTK widgets underneath. Their adaptive behavior still flows through GTK measurement and allocation contracts.
