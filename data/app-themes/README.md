# Waybar colours

The Waybar app-theme switch applies the bundled colour rules to an existing bar. It changes backgrounds, foregrounds and border colours without changing modules, fonts, padding, sizes or positions. Standard modules and workspace states receive palette colours; custom modules receive a general colour treatment. Critical states keep a separate warning treatment.

Deck finds the stylesheet passed to a running Waybar process with `--style` or `-s`. Relative paths resolve against that process's working directory. Otherwise it checks Waybar's user configuration directories and light/dark styles, then the system defaults. A system default is imported through a new user stylesheet, leaving the system file unchanged. Writable symlinked stylesheets retain their symlinks.

The generated `waybar/skwd-theme.css` contains the shipped rules and current palette values. Deck adds an absolute import at the end of each selected stylesheet and requests a Waybar reload when colours change. Imported CSS and the Waybar configuration remain unchanged. Existing palette variable names remain available for setups previously connected to `skwd-colors.css`.

Turning the switch off removes the owned imports and generated file. It preserves unrelated stylesheet edits. If the generated file was edited externally, Deck retains it for review. An interrupted setup can also be switched off. Separate custom outputs can coexist, but an output targeting the managed overlay or connected stylesheet is blocked, including outputs added after setup.

The overlay covers standard Waybar selectors. Unusual selectors with higher specificity, background images, and colour animations may retain parts of their original appearance. Read-only explicit stylesheets require a writable user configuration. The integration does not rewrite arbitrary CSS or execute generated scripts.
