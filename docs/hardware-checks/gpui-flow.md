# gpui-flow canvas check

This procedure checks the spike `tools/spikes/gpui-flow` on GNOME Wayland. The spike is a window with a Graph of 8 nodes and 8 connections on the copied `gpui-flow` canvas. The result of the spike, and what was checked on Xvfb, is in `docs/spikes/gpui-flow.md`. Each step has a pass line.

## Preconditions

- A GNOME Wayland session: `echo $XDG_SESSION_TYPE` prints `wayland`.
- A mouse with a wheel.
- A terminal in the session. The spike prints one line for each release of the mouse button and one line for each zoom change.

## Start

Build the spike, then start it from the repository root:

```sh
cargo build --release --manifest-path tools/spikes/gpui-flow/Cargo.toml
tools/spikes/gpui-flow/target/release/gpui-flow
```

The header of the window shows the theme, the zoom and the number of nodes and connections. The window can come without a title bar: move it with Super and a drag, and close it with Alt+F4 or with Ctrl+C in the terminal. This was not checked on Wayland.

Start the spike again before step 7, so the Graph has its 8 connections.

## Steps

1. Look at the window. All 8 nodes and 8 connections show, and each connection ends on a pin. The header says "8 nodes, 8 connections".
2. Drag the node "Delta T, smooth" by its name to the right and release. The node follows the pointer and its two connections follow the node. The line `release:` in the terminal shows `avg=(x,100)`: x is 560 plus the distance of the drag in px. A drag of 100 px gives `avg=(660,100)`.
3. Drag from the pin at the right of "Ambient temperature" to the pin "B" of "Flow is low" and release. A blue line follows the pointer, and the pin "B" turns blue when the pointer is near it. After the release a connection shows, and the header says "9 connections".
4. Drag from the pin at the right of "Flow" to the pin "A" of "Delta T" and release. This pin has a connection already. No connection shows, and the header still says "9 connections".
5. Drag an empty place of the canvas. All nodes and connections move with the pointer.
6. Put the pointer on the canvas and turn the wheel up until the zoom in the header stops, then down until it stops. It stops at "Zoom 400%" and at "Zoom 25%". The nodes, their text and their pins change size with the zoom, and each connection stays on its pin.
7. Start the spike again. Drag from the pin at the right of "Ambient temperature" to the pin "B" of "Flow is low". Hold the mouse button and press Escape. The blue line goes. Release the button on the pin. No connection shows, and the header says "8 connections".
8. Open GNOME Settings, "Appearance", and set the style "Dark" while the spike runs. The header says "Theme: dark, from the system". The canvas is `#1D1D20`, a node is `#36363A`, and the text is white. Set "Default" again: the canvas and the nodes are white, and the header says "Theme: light, from the system".
9. Leave the pointer at rest on the canvas and run `top -b -d 60 -n 2 -p $(pgrep -x gpui-flow)` in a second terminal. The `%CPU` of the last line is below 5. Write the number in the section "Idle measurement" of `docs/spikes/gpui-flow.md`.

## Record

Date, GNOME Shell version:

| Step | Result |
|---|---|
| 1 | |
| 2 | |
| 3 | |
| 4 | |
| 5 | |
| 6 | |
| 7 | |
| 8 | |
| 9 | |

## More observations

These are not steps of the check. They answer open points of `docs/spikes/gpui-flow.md`, and the owner can use them for the decision of task 1.4.

- Start the spike with `--live` and run the `top` command of step 9. On Xvfb the software renderer took 31% of one core for one frame per second, and the threads of the spike took 1%.
- During a node drag press a key, for example "x", keep the pointer still and release the button. Then move the pointer. On Xvfb the node still follows the pointer until the next click.
- Drag a node and release the button over the header. Then move the pointer over the canvas. On Xvfb the node still follows the pointer until the next click.
- On a touchpad, scroll with two fingers and pinch. Neither was checked.
