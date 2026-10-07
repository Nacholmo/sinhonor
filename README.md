# sinhonor

A portable Rust **motion kit** built from Dishonored (2012): the player's movement
(walk, sprint, crouch, jump, fall, slide, mantle, lean, swim, ladder) and the motion powers
(Blink, Agility), packaged so they can be dropped into other games. It's modelled on how
[2010-rust-rewrite-mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup) brings Skate 3 into MW2.

**No game files ship with this repository.** All tuning (speeds, jump, mantle heights, Blink
range and stepping) is read **at runtime from your own installed copy** of Dishonored.

## Layout

| crate | what it does |
|---|---|
| `crates/upk` | Reader for Dishonored's UE3 packages: LZO chunk flattening, name/import/export tables, tagged properties |
| `crates/dis_data` | Finds your install and loads the motion and Blink tuning from its packages and INI files |
| `crates/dis_motion` | Engine-agnostic movement and Blink logic. The host game supplies collision through a trait |
| `crates/sinhonor_demo` | Bevy test level that shows the motion kit in action |

## Running

```
cargo run --release -p sinhonor_demo -- --game "/path/to/steamapps/common/Dishonored"
```

`--game` defaults to the `DISHONORED_DIR` environment variable, then to common Steam library paths.

## Research notes

`NOTES.md` records what was learned about the file formats and the game's motion and Blink
behaviour, written in our own words. Decompiled code and game data are never committed.

## Credits and licenses

See `NOTES.md` §Credits. This project is dual-licensed under MIT or Apache-2.0.
Dishonored is a trademark of ZeniMax Media; this project is not affiliated with Arkane Studios or Bethesda.
