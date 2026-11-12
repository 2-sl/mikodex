# Mikodex repository instructions

## Git and remote safety

- Never push, publish, or open a pull request against `https://github.com/openai/codex.git`.
- Treat the upstream Codex repository as read-only; fetching releases is allowed.
- Push only to a user-owned remote when the user explicitly asks for that push.
- Make descriptive local Git commits at meaningful milestones.

## Stable-release updates

- When asked to update Mikodex to the latest Codex release, use the newest official stable `rust-v*` source release tag from upstream; exclude prerelease tags.
- Keep Mikodex changes on top of the selected stable release and rebuild the Windows binary.
- Review the resulting changes before committing locally. Never push to the upstream Codex repository.