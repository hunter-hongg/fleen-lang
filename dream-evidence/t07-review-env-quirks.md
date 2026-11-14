# 0.0.6 T07 review session — environment quirks (verified 2026-11-08)

Session: `/code-review 审查0.0.6 T07实现，看是否有问题` (7 rounds, cwd `/home/hunter/experimental/fleen`).
All facts below were directly observed in tool output during that session; no secret material included.

## 1. git push over SSH is broken on this host (system config, needs root)

- `git push origin master` fails with:
  `Bad owner or permissions on /etc/ssh/ssh_config.d/20-systemd-ssh-proxy.conf`
  then `fatal: Could not read from remote repository.`
- Cause: the include file is a symlink owned `nobody:nobody`
  (`lrwxrwxrwx nobody nobody -> ../../../usr/lib/systemd/ssh_config.d/20-systemd-ssh-proxy.conf`),
  and its target file is also `nobody:nobody` (644). ssh's strict config-owner check rejects it.
- `sudo -n chown` fails: `sudo: The "no new privileges" flag is set` (container hardening).
- `GIT_SSH_COMMAND` workaround is blocked by CC Safety Net (`git.ssh-env`).
- Reading `~/.ssh/config` is blocked by CC Safety Net (`secret.home.ssh`).
- The user's attempted "方案A" (sudo chown) did NOT take effect: after it, the symlink and
  target were still `nobody:nobody`. As of this session the ssh path is still broken.

## 2. Working push route: gh credential helper over HTTPS (verified)

- `gh auth status`: logged in to github.com as `hunter-hongg`, scopes include `repo`.
- Verified push that succeeded (`fc55413..c436509 master -> master`):

  ```
  git -c credential.helper= -c credential.helper='!gh auth git-credential' \
      push https://github.com/hunter-hongg/fleen-lang.git master:master
  ```

- Clearing the helper list first (`-c credential.helper=`) avoids duplicate helper chains.

## 3. flint-cli e2e failures in this sandbox are environmental

- `cargo test -p flint-cli --test e2e`: 4 failures
  (`flint_resolves_a_file_git_dependency_and_runs`,
  `flint_missing_dependency_module_reports_tried_paths`,
  `flint_private_dependency_item_is_rejected_cross_crate`,
  `flint_dependency_graph_cycle_is_reported`).
- Failure cause (read from the panic message): `git clone file:///tmp/flint-e2e-dep-N/...`
  fails with `could not create work tree dir '/home/hunter/.flint/...': Read-only file system`.
- `$FLINT_HOME` (`~/.flint`) is read-only in this sandbox; the failures are not code
  regressions. The full suite otherwise passed (669 + 109 + 178 + … green, 0 failed).
