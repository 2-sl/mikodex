# Mikodex repository instructions

## Git and remote safety

- Never push, publish, or open a pull request against `https://github.com/openai/codex.git`.
- Treat the upstream Codex repository as read-only; fetching releases is allowed.
- After committing changes on the active Mikodex branch, immediately push that branch to `origin` when it points to `https://github.com/2-sl/mikodex.git`, unless the user explicitly says to hold the push. Do not force-push.
- Do not push to any other remote unless the user explicitly asks for that push.
- Make descriptive local Git commits at meaningful milestones.

## Stable-release updates

- When asked to update Mikodex to the latest Codex release, use the newest official stable `rust-v*` source release tag from upstream; exclude prerelease tags.
- Keep Mikodex changes on top of the selected stable release and rebuild the Windows binary.
- Review the resulting changes before committing locally. Never push to the upstream Codex repository.
