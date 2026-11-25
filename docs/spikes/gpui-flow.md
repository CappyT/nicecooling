# Spike: the copied `gpui-flow` canvas as the base of the Graph editor

Question: does the node canvas `gpui-flow` (MIT, https://github.com/pacifio/gpui-flow, commit `cfa5787`) work in `gpui-pre =0.3.12` well enough to build the Graph editor on it (design G13 of `openspec/changes/add-node-graph`)?

Code: `tools/spikes/gpui-flow` (outside the workspace, own `Cargo.lock`). `src/main.rs` is the spike program. The other files in `src/` are the copy. Manual check: `docs/hardware-checks/gpui-flow.md`.

The program shows a Graph of 8 nodes and 8 connections. Its options are `--light` and `--dark` (the theme, without them the program follows the system) and `--live` (each node value changes once per second). It prints one line for each release of the mouse button and one line for each zoom change.

## Result

Decision: go

The owner decided on 2026-11-25: the Graph editor is built on the copy. The defects of the section "Defects and missing functions" are the first tasks of the editor.

The copy needs the 11 changes of the section "Changes to the copied code". With them, every behavior of the table works on Xvfb. The owner's checks on GNOME Wayland are in `docs/hardware-checks/gpui-flow.md` and are not done.

Checked on Xvfb, 2026-11-25: Xvfb 21.1.26 at 1280x800, no window manager, the software Vulkan driver lavapipe, release build, rustc 1.100.0. `xdotool` moved the pointer and `import` took the captures. The numbers come from the lines that the program prints. The captures are in `~/.cache/nicecooling-agents/flow-spike/shots/` on the machine of the check. They are not in the repository.

| Behavior | Checked on Xvfb | Capture |
|---|---|---|
| 8 nodes and 8 connections draw, each connection ends on its pin | OK | `01-light-start.png` |
| A node drag of 100 px moves the node by 100 px, and its connections follow | OK: `avg` went from (560,100) to (660,100) | `02-light-node-drag-held.png`, `03-light-node-drag-released.png` |
| A drag from an output pin to a free input pin connects | OK: 9 connections, the new one is `ambient>less.B` | `04-light-connect-held.png`, `05-light-connect-released.png` |
| A drop on an occupied input pin connects nothing | OK: still 9 connections after a drop of `flow` on `sub.A` | `06-light-occupied-held.png`, `07-light-occupied-released.png` |
| A drag on the empty canvas pans | OK: the view went from (0,0) to (-100,-50) | `08-light-pan.png` |
| The wheel zooms and stops at both limits | OK: 8 steps from 100% to 400%, then 16 steps to 25%. More steps change nothing | `09-light-zoom-max.png`, `10-light-zoom-min.png`, `11-light-zoom-43.png` |
| The nodes, the text and the pins follow the zoom | OK at 25%, 43% and 400% | the same three |
| Escape during a connection drag leaves no connection | OK: the draft line goes, and a release on the pin leaves 8 connections. The same drag without Escape gives 9 | `12-light-escape-before.png`, `13-light-escape-after.png`, `14-light-escape-released.png` |
| The look follows the light and the dark tokens | OK with `--light` and `--dark`. A change of the system theme while the program runs is not checked: the shell of the check cannot change it | `01-light-start.png`, `17-dark-start.png`, `18-dark-connect-held.png`, `19-dark-connected-selected.png` |
| The program reads the theme of the system at start | OK: without an option it showed "dark, from the system", the setting of the desktop session of the machine | `00-baseline-upstream.png` |
| Delete removes the selected node and its connections | OK: 7 nodes and 6 connections | `20-dark-delete-node.png` |
| Ctrl+Z | Does nothing. Super+Z brings the node back | `21-dark-ctrl-z.png`, `22-dark-super-z.png` |

## Idle measurement

Setup as above. The program ran 11 s before each sample. The sample is 60 s with the pointer at rest on the canvas. CPU is the `utime + stime` delta of `/proc/<pid>/stat` (tick = 10 ms), and the value of `top -b -d 60 -n 2`.

| Run | CPU in 60 s | `top` | `VmRSS` | Threads |
|---|---|---|---|---|
| `--dark` | 5 ticks, 0.08% of one core | 0.1% | 183640 KiB | 176 |
| `--dark --live` | 1901 ticks, 31.7% of one core | 31.2% | 194064 KiB | 176 |

- The canvas has no timer and no animation: without `--live` it draws no frame at rest.
- With `--live` the program draws one frame per second. In a sample of 20 s the threads of the software rasterizer (`llvmpipe-*`) took 612 of 631 ticks. The threads of the program took 19 ticks: 0.95% of one core, about 9.5 ms per frame. The 31.7% is the cost of software rendering on Xvfb. The cost with a GPU is a check of the owner.
- The memory includes the software Vulkan driver.

## The copy before the changes

Measured on Xvfb with the unchanged copy and the same program:

- The connections draw 46 px above their pins when the canvas does not start at the top left corner of the window (`00-baseline-upstream.png`). The program has a header of 46 px above the canvas. The edge painter and the pointer math use window coordinates, and the nodes use coordinates of the canvas.
- All pins of one side of a node are at the middle of that side, one over the other (`00-baseline-upstream.png`).
- The zoom moves the nodes and does not scale them: at 40% the nodes overlap (`00-baseline-zoom-40.png`).
- The wheel alone pans by 60 px for each step. Ctrl and the wheel zoom by 60 points for each step (100%, 40%, 25%), and wheel up zooms out.
- The default limits are 80% and 250%. They are public fields, and the program sets 25% and 400%.

Read in the code of the copy, not run:

- `FlowGraph::validate_connection` and `FlowGraph::on_connect` store their function, and no code calls it.
- No key cancels a connection drag.

## Changes to the copied code

`git diff dd52ff1 -- tools/spikes/gpui-flow` shows them: 272 lines added and 178 removed in `src/graph.rs`, `src/store.rs`, `src/types.rs` and `src/edges/mod.rs`. The files `src/lib.rs`, `src/controls.rs`, `src/minimap.rs`, `src/edges/bezier.rs`, `src/edges/smooth_step.rs` and `src/edges/straight.rs` are the files of the source commit.

Not copied: `examples/` (they need `gpui_macos`), `README.md`, `media/`, `.gitignore` and the `Cargo.lock` of upstream. The `Cargo.lock` of the spike starts from the lock file of the workspace, so the spike builds the GPUI of the app.

| # | Change | Reason |
|---|---|---|
| 1 | `Cargo.toml`: the dependency `gpui` from the Zed git repository becomes `gpui-pre = "=0.3.12"`. New dependency `gpui-pre-platform` with `wayland` and `x11`. New `[workspace]` table and `publish = false` | The pinned GPUI of the app. `gpui-pre-platform` has the entry point of an application. The copy stays out of the root workspace |
| 2 | `Cargo.toml`: removed the dev-dependency `gpui_macos`, the three `[[example]]` entries, the `[profile.dev]` tables and the keys `homepage`, `readme`, `keywords` and `categories` | They belong to files that are not copied, or to packages that the pinned GPUI does not have |
| 3 | `FlowState::origin` and `FlowState::to_canvas`. The canvas stores its origin at each paint. Every mouse handler changes the pointer position to canvas coordinates, and the edges, the draft line and the selection box add the origin when they paint | Connections drew above their pins, and a snap target was 46 px above its pin, when the canvas was not at the window origin. The editor is next to a sidebar and under a header bar |
| 4 | `HandleDef::offset` and the function `handle_center` in `store.rs`, which replaces `handle_center_from_node` in `edges/mod.rs` and the body of `find_handle_center`. A pin with an offset has a hit area of its own size | A node of the Graph model can have several input pins on one side. Each pin needs its own place, next to its label |
| 5 | The snap highlight is on the pin that the draft snaps to, not on every input pin of that node | With more than one input pin the user must see which pin takes the connection |
| 6 | The canvas calls the function of `validate_connection` before it adds a connection, and the function of `on_connect` in place of its own add. Both are `Rc` | Upstream never calls them. The rule "an input pin has at most one connection" is a rule of the app, and the app gives the connection its id |
| 7 | Removed the release handler of a pin. The release handler of the canvas completes every connection from the snap target | Read in the code: the handler of a pin did not check the pin type, so a drag from an output pin to an output pin made a connection. One path also means one place for the validator |
| 8 | Escape cancels a connection drag | Required by task 1.3. Upstream cancels only by a release away from a pin |
| 9 | `FlowState::zoom_on_scroll` (default off, the upstream behavior). A wheel step multiplies the zoom by 1.2, and wheel up zooms in. `on_scroll_wheel` becomes `on_scroll` | Required by task 1.3. The upstream step of 60 points went from 100% to 40% in one step. `on_scroll_wheel` is deprecated in `gpui-pre` 0.3.12 |
| 10 | The element `Zoomed` in `graph.rs` lays out and paints each node with a rem size multiplied by the zoom. The measured size of a node is in flow units. The pin size follows the zoom | Upstream did not scale the nodes. A node content that gives its sizes in rems now follows the zoom. The measured size no longer changes with the zoom, so the edges stay on the pins in the frame of a zoom change |
| 11 | `FlowGraph::accent_color` and `FlowGraph::edge_color` replace the fixed colors `0x3b82f6`, `0x1d4ed8`, `0x93c5fd`, `0xb1b1b7` and `0x555555` | The selection, the snap target, the draft line and the edges take the tokens |

## Defects and missing functions

Measured on Xvfb, with the changed copy:

- **A release after a key press is lost.** A key press during a node drag, then a release with no pointer motion between them: the canvas does not see the release, and the node follows the pointer until the next click (`15-light-key-during-node-drag.png`). With 2 px of motion after the key the release works. The cause is not measured. Two facts from the code point to the hover state: the release handler of the canvas runs only when its area is hovered, and the X11 client of `gpui-pre` hides the pointer after a key press until the pointer moves. A connection drag that Escape cancelled is not affected. **Fixed in `crates/nicecooling-flow`.** The cause is the second fact: `Hitbox::is_hovered` of `gpui-pre` 0.3.12 returns false after a key press until the next mouse move, and the release handler of an element needs it. The canvas now ends every drag with a window listener, which needs no hover. The test is `release_after_a_key_press_ends_a_node_drag`.
- **A release outside the canvas is lost.** A node drag that ends over the header leaves the drag active (`16-light-release-outside-canvas.png`). A pan ends in the same handler: not run. A release listener on the window, in place of the listener on the canvas area, is the usual GPUI answer to both defects. **Fixed in `crates/nicecooling-flow`** with that listener. The tests are `release_outside_the_canvas_ends_a_node_drag` and `release_outside_the_canvas_ends_a_pan`. Escape now also cancels a node drag and puts the node back (`escape_cancels_a_node_drag_and_puts_the_node_back`), and a press on the canvas gives it the focus.
- **An occupied pin looks free during the drag.** The validator runs at the release. Before it, the occupied pin has the highlight of a snap target (`06-light-occupied-held.png`). The validator returns true or false: the canvas has no place for the reason of a refusal, which task 8.4 needs. **Fixed in `crates/nicecooling-flow`.** The rule of the app returns `Err(reason)`. The canvas runs it while the draft is over a pin: the pin shows the refused look and the draft line takes the danger color. At the release the canvas adds nothing and sends `FlowEvent::ConnectionRefused` with the reason. The tests are `the_draft_over_an_occupied_pin_has_the_reason_before_the_release`, `a_release_on_an_occupied_pin_gives_the_reason`, `a_drop_on_an_occupied_pin_sends_the_reason_to_a_subscriber` and `a_refused_snap_target_shows_the_refused_look`.
- **Undo is on the wrong key for Linux.** Undo, redo, select all and the add to a selection use the `platform` modifier, which is the Super key on Linux. Ctrl+Z does nothing. **Fixed in `crates/nicecooling-flow`** by removal: the canvas binds no undo key and keeps no history. The test is `the_canvas_binds_no_undo_key`. Select all, the add to a selection and the box selection went too: the editor has one selection.
- **The selected node moves by 1 px.** Its border goes from 1 px to 2 px (`22-dark-super-z.png`).
- **Connections pass behind nodes.** There is no routing (`01-light-start.png`, `water` to `switch.On`).
- **Line width and arrow size do not follow the zoom.** At 25% the arrows are large next to the nodes (`10-light-zoom-min.png`). At 400% the pin hides the arrow.
- **The grid dots are uneven at some zoom values** (`11-light-zoom-43.png`).
- **The draft line is dim in dark** (`18-dark-connect-held.png`): the accent at 50% opacity on the `view` ground.
- **A value change repaints the whole canvas.** The cost of one frame was 9.5 ms of CPU in the threads of the program for 8 nodes.

Read in the code, not run:

- **Keyboard.** The canvas has no focus ring, no Tab order, no move of a node with the arrow keys and no way to connect two pins without a pointer. `docs/ui/DESIGN.md` asks for a focus ring on every focusable element.
- **Accessibility.** A node and a pin have no role and no name. `gpui-pre` 0.3.12 has a role hook on `Element`, and the copy does not use it.
- **Pin hit area.** A pin with an offset is 10 px wide at 100% and 2.5 px at 25%. The snap radius of 20 px helps at the end of a drag, not at its start.
- **Undo history of the canvas.** A press on a node stores a full copy of all nodes and connections, also for a click with no move. The history holds 100 copies. **Fixed in `crates/nicecooling-flow`:** the history is gone. The editor of task 8.3 owns undo.
- **Connection ids.** Without `on_connect` the id of a new connection is `e<source>-<target>`: two connections between the same two nodes get the same id. **Fixed in `crates/nicecooling-flow`:** `on_connect` is gone. The canvas gives each connection a unique id and sends `ConnectionAdded`, and the app sets its own state.
- **Change events.** `NodeChange`, `EdgeChange` and their apply functions exist, and no code sends them. The handlers change `FlowState` without a notify: an observer must observe the `FlowGraph` entity. **Fixed in `crates/nicecooling-flow`:** `FlowState` sends a `FlowEvent` for each input, and the state notifies its observers. The crate documentation describes the API. The tests are `a_drag_sends_one_node_moved_event`, `a_connection_sends_connection_added`, `the_canvas_keeps_one_selection`, `delete_removes_a_node_and_its_connections_and_tells_the_app` and `events_reach_a_subscriber_in_the_order_of_the_inputs`.
- **Theme change.** The colors and the node renderers are set when `FlowGraph` is built. The program builds a new `FlowGraph` on a theme change. The view and the selection are in `FlowState` and stay.
- **Culling** uses the size of the window, not of the canvas.
- **Frame cost.** Every frame clones each visible node and builds every element again. A pointer move during a drag repaints the whole canvas.
- **`Controls` and `Minimap`** have fixed dark colors and take the canvas size as a fixed number. The spike does not use them.

Not checked: a Graph of 256 nodes, the pinch zoom, a touchpad scroll, the box selection (Shift and drag), the selection of several nodes, the selection of a connection, the edge types `SmoothStep` and `Straight`, an input control inside a node, `Controls` and `Minimap`.

## API gaps for the Graph editor

| Need of the editor | State of the copy |
|---|---|
| Custom node content with live values | Works: a renderer per node type returns any element. Checked with `--live`. The renderer gets the `FlowNode` only, so the app keeps its own data by node id. The content must give its sizes in rems. A new value needs a notify on the `FlowGraph` entity, which repaints all nodes |
| Pins with labels | Works with change 4. The app gives each pin its offset in flow units and draws the label itself. The canvas cannot place a pin next to a child element, so the app must know the row heights of its node content |
| Parameter panel | Not part of the canvas. The app builds it next to the canvas. The canvas gives the selection only as the `selected` flag of each node in `FlowState`: there is no selection event |
| Validation of a connection by our rules | Works with change 6, at the release only. Missing: the reason of a refusal, and the validation while the draft is over a pin |
| Undo | The canvas has its own history and its own keys. The draft of the editor (task 8.3) must own the history, so the history of the canvas must go, and the canvas must report the end of a node drag, a delete and a selection change. Only `on_connect` exists |
| Delete except the end node | `FlowNode::deletable` exists. Not checked |
| Add a node at a position | `Viewport::screen_to_flow` and `FlowState::to_canvas` give the position. There is no drop target |
| Tests without a display (task 8.1) | The connect, zoom and pan logic is in closures of `FlowGraph::render`. The four tests of task 8.1 need this logic as functions of `FlowState` |
| Lints of the workspace (task 8.1) | `cargo clippy` with its default lints gives 29 warnings on the copy. The copy is not formatted with `rustfmt` |

## Build notes

- `cargo check` of the copy takes 31 s and the release build 68 s on the machine of the check, with every dependency already downloaded.
- The window options are the defaults of GPUI. The app uses the options of `gpui_component::TitleBar`. The spike has no dependency on `gpui-component`.
- A length in the node content of `src/main.rs` goes through `units()`, which gives rems. A length in `px` does not follow the zoom.

## Open items

- The owner runs the procedure in `docs/hardware-checks/gpui-flow.md` and records the idle CPU here.
