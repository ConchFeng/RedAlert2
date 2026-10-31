---
name: release
description: Build or publish RedAlert2 installers through its existing GitHub Release workflow when requested.
---

# Release

Use repository `ConchFeng/RedAlert2` explicitly with `gh --repo` or API paths.
The workflow at [.github/workflows/release.yml](../../../.github/workflows/release.yml)
owns packaging, platform checks and promotion. A request to compile locally does
not authorize a release or a remote workflow run.

For a requested development build:

```sh
gh workflow run release.yml --repo ConchFeng/RedAlert2 --ref main -f publish=none
```

Use `publish=draft` for a requested reviewable release draft and
`publish=prerelease` when publication is authorized. Identify the dispatched run and
wait once with `gh run watch <run> --repo ConchFeng/RedAlert2 --exit-status`.

For a stable release, select a tested published development build. Create the
requested annotated version tag on that exact commit and dispatch the same
workflow with `-f promote=<dev-build-tag>`. Publishing tags and the draft requires
the user's release authority; respect any no-commit or no-push restriction.

Keep the release page to the three platform installers, a short list of visible
changes, the compare link, and the install/license lines produced by
[notes.sh](../../../.github/release/notes.sh). Verify claims against the included
changes. Preserve contributors' credit without adding notification mentions.

Check the published version, assets and provenance:

```sh
gh attestation verify <installer> --repo ConchFeng/RedAlert2
```

Website/R2 publication is a separate configured service, documented in
[downloads.md](../../../.github/release/downloads.md). Verify that its endpoint,
signing key, bucket, credentials and environment belong to this fork before using
it; inherited upstream domains are not evidence of fork ownership. Do not create
secrets, approve deployments or redirect update traffic without that authority.

Raise the development version afterward only when requested, keeping both package
manifests and their lockfile entries aligned. Never delete releases/tags, change
repository rules or roll back an update feed without explicit authorization.
