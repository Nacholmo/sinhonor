# AGENTS.md — read this first. These rules are absolute.

Breaking either rule is not a mistake to apologise for afterwards. It is a
publication that cannot be taken back. If you are unsure whether an action
breaks a rule, **it does. Stop and ask the owner.**

## NEVER ship game assets

"Game assets" means any byte that comes from, or is derived from, an install of
Call of Duty (any title), Dishonored, or any other game. That includes the files
themselves (`.ff`, `.iwd`, `.iwi`, `.gsc`, `.upk`, `.u`, `.tfc`, `.pck`, `.bnk`,
`.wem`, `.xex`, saves, videos) **and anything extracted, decoded or converted
from them**: textures, meshes, skeletons, animations, sounds (`.ogg`, `.wav` or
raw PCM), particle systems, shaders, scripts, string tables, maps, collision,
tuning tables, localisation, and everything under `iw4l-artifacts/`.

"Ship" means any of: commit, push, put in a release archive, upload as a release
asset, artifact, gist or paste, attach to an issue or PR, or **embed in source**
(`include_bytes!`, base64, hex or number arrays, long string literals, test
fixtures). Both games are read from the player's own install at runtime. That is
the only way game data enters this program, and it never leaves the player's
machine.

- Do not dump data from an install into source to avoid reading it at runtime.
  If a value comes from the install, it is read from the install.
- Rendered screenshots are not assets, but they show game content. They go only
  in `docs/screenshots/`, as small JPEGs, and **only with the owner's explicit
  approval of that image in the current session**.
- A release archive is built from source and holds the binary, licences and docs.
  List its contents and check them before uploading anything.
