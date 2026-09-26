# Waybar colours

The Waybar app-theme switch applies the bundled colour rules to an existing bar. It changes backgrounds, foregrounds and border colours without changing modules, fonts, padding, sizes or positions. Standard modules and workspace states receive palette colours; custom modules receive a general colour treatment. Critical states keep a separate warning treatment.

Deck finds the stylesheet passed to a running Waybar process with `--style` or `-s`. Relative paths resolve against that process's working directory. Otherwise it checks Waybar's user configuration directories and light/dark styles, then the system defaults. A system default is imported through a new user stylesheet, leaving the system file unchanged. Writable symlinked stylesheets retain their symlinks.

The generated `waybar/skwd-theme.css` contains the shipped rules and current palette values. Deck adds an absolute import at the end of each selected stylesheet and requests a Waybar reload when colours change. Imported CSS and the Waybar configuration remain unchanged. Existing palette variable names remain available for setups previously connected to `skwd-colors.css`.

Turning the switch off removes the owned imports and generated file. It preserves unrelated stylesheet edits. If the generated file was edited externally, Deck retains it for review. An interrupted setup can also be switched off. Separate custom outputs can coexist, but an output targeting the managed overlay or connected stylesheet is blocked, including outputs added after setup.

The overlay covers standard Waybar selectors. Unusual selectors with higher specificity, background images, and colour animations may retain parts of their original appearance. Read-only explicit stylesheets require a writable user configuration. The integration does not rewrite arbitrary CSS or execute generated scripts.

# Fish colours

The Fish app-theme switch adds a managed source block at the end of `fish/config.fish` and generates `fish/skwd-colors.fish`. It sets syntax, prompt and completion colours from the current palette, independently of the terminal's palette.

Open a new Fish session after enabling it for the first time. That session then receives colour changes through a Fish universal-variable event and repaints its command line. Deck starts a bounded, non-interactive Fish process to publish the event, so normal non-interactive Fish startup configuration runs. Sessions opened before setup receive the integration when they next load their configuration.

The integration keeps each session's previous global colour values and leaves its universal colour variables unchanged. Turning it off removes the managed config block and restores the saved globals in connected sessions, including empty values and export flags. Custom prompts with hard-coded colours can keep those colours. Changes to the managed file or source block stop automatic file updates and remain intact on removal.

# KDE recovery

A failed KDE colour refresh is retried at the next theme apply. The saved transaction records its intended output and selected scheme so recovery can finish a write or retry the KDE tool without overwriting externally changed theme files or selections. A failed undo retries restoration of the previous scheme, rather than enabling the managed theme again. No background retry timer runs.

# Custom mappings and recovery

Every built-in app theme has an optional editable template at `$XDG_CONFIG_HOME/skwd-wall-v2/app-themes/<id>.template`. In Colours, expand the app's details and choose **Create editable template**. Copy the displayed path, edit the file, save it, then choose **Refresh colours**. The daemon reads the override on every colour apply; upgrades never overwrite it. **Reset mappings to defaults** backs up and removes the override. Refresh colours to apply the defaults.

KDE, Kitty, Fish, Ghostty, btop, Rofi, Niri and Waybar use their native template syntax. For example, map KDE's selection background to `secondary` by changing the red, green and blue tokens on `BackgroundNormal` in `[Colors:Selection]` from `colors.primary.default.*` to `colors.secondary.default.*`. Literal colours are also allowed. Edit the template rather than the generated `.colors` file. Unsupported colour tokens fail without publishing an unresolved template; an invalid Niri configuration is rejected by `niri validate`.

VS Code, Alacritty and Yazi use a JSON list of field paths and palette roles. For example, Alacritty's template can contain:

```json
[
  {"path": ["colors", "primary", "background"], "role": "surface"},
  {"path": ["colors", "cursor", "cursor"], "role": "secondary"}
]
```

The template created by Skwd contains every default mapping. Changing a role changes that field on subsequent applies. Removing a field restores its previous value and stops controlling it. Duplicate paths and unknown roles are rejected. VS Code mappings stay within `workbench.colorCustomizations`, Alacritty within `colors`, and Yazi maps `fg` and `bg` fields. Unrelated settings remain intact.

**Disconnect and keep current colours** stops managed updates without modifying application configuration, generated files or the selected KDE scheme. It works even when those files were edited or removed. The disconnected state survives daemon restarts. **Back up edits and reconnect** saves current files and receipts under `$XDG_STATE_HOME/skwd-wall-v2/app-themes/backups`, then resumes updates using the template. Direct edits to generated colours are replaced only by this explicit action. A failed reconnect remains disconnected so wallpaper changes cannot continue a partial reconnect. A manually selected KDE scheme becomes the scheme restored on the next normal disable.

The ordinary on/off switch still restores the settings from before setup where ownership is intact. Disconnect is the alternative when current colours or manual edits should remain. Read-only and declaratively managed application files still require changes in their owning configuration; disconnect itself needs only writable Skwd state.

The audit covers all eleven managed integrations. Their previous lockouts came from generated-file equality checks, missing import blocks, changed structured colour fields, or a different KDE selection. Those ownership checks remain in automatic updates; all eleven now have explicit disconnect/reconnect and customization paths. Custom output integrations already use user-selected templates, and shell providers own their settings independently; neither uses these managed receipts.
