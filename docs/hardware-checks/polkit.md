# polkit check

This procedure checks the polkit policy and rule of nicecoolingd with `pkcheck` on the reference desktop. It has 3 cells: 3 actions in the active session of the owner. Each cell has one pass line.

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
| `io.github.cappyt.NiceCooling1.critical` | auth_admin_keep |

## Record

| Cell | Result |
|---|---|
| write | |
| cancel-poweroff | |
| critical | |
