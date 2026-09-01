pub(super) use wall_proto::helm::VERBS;

pub fn run() -> i32 {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let Some(verb) = args.first().cloned() else {
        print_help();
        return 0;
    };
    if verb == "--help" || verb == "-h" {
        print_help();
        return 0;
    }
    if verb == "--version" || verb == "-V" {
        println!("skwd-helm {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }
    if !VERBS.contains(&verb.as_str()) {
        eprintln!("skwd-helm {verb}: unknown verb '{verb}'");
        return 4;
    }
    args.remove(0);
    match super::commands::run(&wall_proto::resolve_socket(), &verb, args) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("skwd-helm {verb}: {}", error.message());
            error.code()
        }
    }
}

pub(super) fn print_help() {
    println!(
        "skwd-helm - control client for Skwd Deck and Skwd Wall\n\
\n\
USAGE:\n\
  skwd-helm <verb> [args]         control the running daemon or picker\n\
\n\
VERBS:\n\
  apply <path|key> [-o OUT]       apply a wallpaper by file path or library key/name\n\
  retheme                         regenerate and reload app colours without changing wallpaper\n\
  random [-o OUT] [--favourites] [--type T] [--tag T]\n\
                                  apply a random wallpaper (optionally filtered)\n\
  next | prev [-o OUT]            advance / rewind the playlist on an output\n\
  back | forward [-o OUT]         step through wallpaper history on an output\n\
  history [-o OUT] [--json]       show per-output wallpaper history and position\n\
  list [--json] [--favourites]    list library wallpapers (key, type, name)\n\
  current [--json]                show the current wallpaper per output\n\
  outputs [--json]                show per-output state (type, mute, volume, current)\n\
  displays [--json]               enumerate physical outputs through Wayland\n\
  pause | resume                  freeze / unfreeze animated wallpapers\n\
  mute | unmute [-o OUT]          mute / unmute wallpaper audio\n\
  volume <0-100> [-o OUT]         set wallpaper audio volume\n\
  favourite <key> [--off]         mark / unmark a wallpaper as favourite\n\
  export [--out DIR] [--name N] [--include-library] [--assets reference|bundle]\n\
                                  write a shareable look pack (portable config + palette)\n\
  import <DIR> [--merge|--replace] [--dry-run] [--allow-hooks]\n\
                                  apply a look pack; hooks quarantined unless --allow-hooks\n\
  watch [--exec 'cmd %path%']     stream daemon events as JSON; run a hook per event\n\
  ui <cmd>                        drive the RUNNING picker UI:\n\
  toggle | show | hide | close | dismiss\n\
                                    mode <slices|hex|wall|sandy>\n\
                                    filter <text> | clear\n\
  open <settings|effects|mixer|downloads|tags|playlists|schedule|theme|theme-bar>\n\
  playlist <id>                    select a playlist in the open workbench\n\
  playlist-demo <action>            build an in-memory playlist during a demo session\n\
  nav <left|right|up|down> | pick\n\
  select <wallpaper-key> | flip\n\
  kind <any|static|video|we> | orient <any|wide|tall>\n\
  resolution <any|WIDTHxHEIGHT> | bar <show|hide|toggle>\n\
  demo <begin|end> | tune <config.path> <value> | motion <scale>\n\
  recolour <palette> | section <index> | sort <mode> | colour <name|any>\n\
  apply <output> [shader] [duration-ms]  (demo session only)\n\
  set <config.path> <value>   (e.g. set transition.durationMs 1200)\n\
                                    state   (print picker state as JSON: mode, selection,\n\
                                             camera, filter, overlay, animation flags)\n\
  hw-probe                        print GPU capability report as JSON (adapters,\n\
                                  backend, BC7 thumbnail support, texture limits)\n\
\n\
EXIT CODES: 0 ok, 1 rpc error, 2 not found, 3 daemon unreachable, 4 bad args,\n\
            5 unknown method (older daemon), 6 invalid params\n\
ENV: SKWD_WALL_V2_SOCK overrides the daemon socket path"
    );
}
