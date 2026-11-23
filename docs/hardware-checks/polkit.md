# polkit check

This procedure checks the polkit policy and rule of nicecoolingd with `pkcheck` on the reference desktop. It has 12 cells: 3 actions in 4 session kinds. Each cell has one pass line.

## Preconditions

- Fedora 45 on the reference desktop. Two users exist: `owner` (a `wheel` member) and `plain` (not in `wheel`).
- The files are installed by hand:
  `sudo install -m 644 packaging/polkit/io.github.cappyt.NiceCooling1.policy /usr/share/polkit-1/actions/` and
  `sudo install -m 644 packaging/polkit/50-nicecooling.rules /usr/share/polkit-1/rules.d/`.
  `pkaction --action-id io.github.cappyt.NiceCooling1.write` prints the action.
- Four sessions. A: `owner` at the graphical console, the active session. B: `owner` in a second local session that is not active (switch the console to another user, or lock it with fast user switching). C: `plain` in a local session. D: `owner` in an SSH session.
- `pkcheck` needs the process id of the session shell. In each session run `echo $$` and use that value as `PID`.

## Command

For each cell run, in the session of the cell: `pkcheck --action-id <action> --process $$ --allow-user-interaction=no; echo "exit=$?"`.

- Exit 0 means YES (authorized without a password).
- Exit 3 means the action needs authentication (`auth_admin_keep`). The text `Authorization requires authentication` also shows.
- Exit 1 means no.

## Matrix

| Action | A: wheel, active | B: wheel, inactive local | C: non-wheel | D: SSH |
|---|---|---|---|---|
| `io.github.cappyt.NiceCooling1.write` | YES | auth_admin_keep | auth_admin_keep | auth_admin_keep |
| `io.github.cappyt.NiceCooling1.cancel-poweroff` | YES | YES | auth_admin_keep | auth_admin_keep |
| `io.github.cappyt.NiceCooling1.critical` | auth_admin_keep | auth_admin_keep | auth_admin_keep | auth_admin_keep |

Session C is a local session of a user who is not in `wheel`. For the column "non-wheel" run the cell in the active session of `plain`.

## Record

| Cell | Result |
|---|---|
| write, A | |
| write, B | |
| write, C | |
| write, D | |
| cancel-poweroff, A | |
| cancel-poweroff, B | |
| cancel-poweroff, C | |
| cancel-poweroff, D | |
| critical, A | |
| critical, B | |
| critical, C | |
| critical, D | |
