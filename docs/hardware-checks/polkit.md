# polkit check

This procedure checks the polkit policy and rule of nicecoolingd with `pkcheck` on the reference desktop. It has 3 cells for the 3 actions in the active session of the owner, and one manual step for the Graph sensor members with a user outside `wheel`. Each cell and the step have one pass line.

## Preconditions

- Fedora 45 on the reference desktop. `owner` is a `wheel` member.
- The files are installed by hand:
  `sudo install -m 644 packaging/polkit/io.github.cappyt.NiceCooling1.policy /usr/share/polkit-1/actions/` and
  `sudo install -m 644 packaging/polkit/50-nicecooling.rules /usr/share/polkit-1/rules.d/`.
  `pkaction --action-id io.github.cappyt.NiceCooling1.write` prints the action.
- The check runs in the active graphical session of `owner`.

## Command

For each cell run: `pkcheck --action-id <action> --process $$; echo "exit=$?"`. Without `--allow-user-interaction`, `pkcheck` asks for no password.

- Exit 0 means YES (authorized without a password).
- Exit 2 means the action needs authentication: stdout shows `polkit\56result=auth_admin_keep` (`polkit.result`).
- Exit 1 means no.

## Matrix

| Action | Expected |
|---|---|
| `io.github.cappyt.NiceCooling1.write` | YES |
| `io.github.cappyt.NiceCooling1.cancel-poweroff` | YES |
| `io.github.cappyt.NiceCooling1.critical` | YES |

## Record

| Cell | Result |
|---|---|
| write | |
| cancel-poweroff | |
| critical | |

## Graph sensor members with a user outside `wheel`

The polkit mock of the tests knows no users, so this step runs by hand. The daemon asks the action `io.github.cappyt.NiceCooling1.write` at every call of `CreateGraphSensor`, `SetGraphSensorValues`, `GraphSensor.SetDefinition` and `GraphSensor.Delete`. The values call carries no interactive flag, so a user without the right gets an error and no dialog. The create call carries the flag, so the same user gets the dialog.

Preconditions:

- `nicecoolingd` runs. `guest` is a local user outside `wheel` (`id guest` lists no `wheel`) with a graphical session of its own. The step runs in a terminal of that session.
- `B=io.github.cappyt.NiceCooling1` and `R=/io/github/cappyt/NiceCooling1`.

Steps:

1. As `owner`: `busctl --system call $B $R $B.Manager CreateGraphSensor ss "Polkit check" degC`. It prints `o "/io/github/cappyt/NiceCooling1/sensor/graph_<n>"` and shows no dialog. Note `graph_<n>`.
2. As `guest`: `busctl --system --allow-interactive-authorization=false call $B $R $B.Manager SetGraphSensorValues 'a(sbd)' 1 graph_<n> true 1.0; echo "exit=$?"`. Expected: `busctl` prints the error `Access denied` (`org.freedesktop.DBus.Error.AccessDenied`) and `exit=1`. No password dialog shows.
3. As `guest`: `busctl --system --allow-interactive-authorization=true call $B $R $B.Manager CreateGraphSensor ss "Polkit check 2" degC; echo "exit=$?"`. Expected: the polkit dialog shows with the text "Authentication is required to change cooling settings". Cancel the dialog. `busctl` prints `Access denied` and `exit=1`.
4. As `owner`: `busctl --system get-property $B $R/sensor/graph_<n> $B.Sensor HasValue` prints `b false`, so the denied call of step 2 set no value. Then remove the sensor: `busctl --system call $B $R/sensor/graph_<n> $B.GraphSensor Delete`.

The step passes when all 4 steps show the expected result. The owner writes `pass` or `fail` in the row below after the run.

| Step | Result |
|---|---|
| graph sensor members, user outside `wheel` | |
