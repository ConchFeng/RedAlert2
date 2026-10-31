---
name: run-game
description: Build and launch RedAlert2's ordinary VERA20k game when the user asks to run, start or play it.
---

# Run the game

Resolve the current checkout and check whether its game is already running before
starting another instance. Use its normal menu and configuration.

Build through the shared runner:

```sh
python -m tools.cargo_run -- build --locked --profile fast -p vera20k --bin vera20k
python -m tools.cargo_run --resolve vera20k --profile fast
```

Use `--release` and resolve the `release` profile when the user requests performance
measurement. Default to an unlabelled build; preserve a labelled binary only when
the task needs a saved build or an A/B comparison. Follow the existing
[build owner](../../../tools/README.md#cargo-ownership-and-labeled-builds).

Launch the resolved executable from the configured checkout so `config.toml` is
found. Use the primary checkout only when it owns the required retail configuration.
Preserve `RA2_DIR` if it supplies the data path and remove other inherited `RA2_*`
overrides from the child environment. Do not alter the caller's environment.

Confirm a responsive visible game window and leave the requested session running.
Report an existing session or missing retail configuration accurately. Automated
captures, quickplay and developer modes require their own task scope.
