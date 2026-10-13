# Licensing

Original Skwd Deck code, including `skwd-steam`, is licensed under MIT;
see [LICENSE](LICENSE). Third-party code retains its upstream licences.
Licence texts are in [LICENSES](LICENSES).

`skwd-steam` links Valve's proprietary `libsteam_api.so`. The MIT licence
covers the Skwd helper code, not Valve's runtime. Redistribution of that
runtime remains subject to the Steamworks SDK Access Agreement.

`skwd-wall-scan` depends on Paper's `paper-scene` crate, which currently
retains its GPL licence. Deck's MIT licence does not remove those
obligations from a scanner build that links Paper. Review the linked dependencies and native
FFmpeg build when distributing each executable.

This change does not relicense earlier releases.
