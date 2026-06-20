# Merge request template

Use for **`gh pr create`** and **`gh pr edit`**. Base content on **full branch** scope (`git log main..HEAD`, `git diff main...HEAD`), not just the latest commit.

## Title

`(<scope>) Short imperative description`

## Summary

- 

## Technical notes

- 

## Testing

- [ ] `task test` from repo root (CI parity; see `.ai/skills/build-via-task/SKILL.md`)
- [ ] Narrow `cargo test -p …` / `npm run …` if used for local iteration (list commands)

## Checklist

- [ ] Crypto / security impact reviewed (`SECURITY.md`)
- [ ] FFI/UniFFI impact called out if applicable (**meta-secret-compose**)
- [ ] No secrets or keys committed

## Related

- Closes #issue (if applicable)
