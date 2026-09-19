These sink-input fixtures retain the classification fields from real PipeWire Pulse captures on 2026-09-19.

- `mpv-skwd-title.json`: mpv playing an AAC tone with `--force-media-title=skwd-demo.mp4`. The private sink monitor measured RMS 0.0856 while this stream was active.
- `paper-alsa.json`: the shipped beta.16 `skwd-wall-vk` native scene mixer in a private Plasma session. Video playback uses the same application and node identity. This ALSA route supplies neither the process binary nor its PID.

Media titles describe the playing content. They cannot establish which application owns a stream.
