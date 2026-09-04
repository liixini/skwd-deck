%global skwd_version 1.0.0~beta.6

Name:           skwd-deck-steamworks
Version:        %{skwd_version}
Release:        1%{?dist}
Summary:        Optional Steam Workshop runtime for Skwd Deck
License:        LicenseRef-Proprietary
URL:            http://192.168.1.41:3000/liixini/skwd-deck
Source0:        skwd-steam
Source1:        libsteam_api.so
BuildArch:      x86_64
Recommends:     skwd-deck = %{version}

%description
Transient Steamworks-backed Workshop helper for Skwd Deck. The Valve runtime
is redistributed under the Steamworks SDK Access Agreement. The package does
not install, start, or enable Steam or a service.

%prep

%build

%install
install -Dpm0755 %{SOURCE0} %{buildroot}%{_libexecdir}/skwd-deck/skwd-steam
install -Dpm0755 %{SOURCE1} %{buildroot}%{_libexecdir}/skwd-deck/libsteam_api.so
mkdir -p %{buildroot}%{_bindir}
ln -s ../libexec/skwd-deck/skwd-steam %{buildroot}%{_bindir}/skwd-steam

%files
%{_bindir}/skwd-steam
%{_libexecdir}/skwd-deck/skwd-steam
%{_libexecdir}/skwd-deck/libsteam_api.so

%changelog
* Fri Sep 04 2026 Skwd maintainers <noreply@local> - 1.0.0~beta.6-1
- Prepare coordinated release 1.0.0-beta.6.
* Fri Sep 04 2026 Skwd maintainers <noreply@local> - 1.0.0~beta.5-1
- Prepare coordinated release 1.0.0-beta.5.
* Thu Sep 03 2026 Skwd maintainers <noreply@local> - 1.0.0~beta.4-1
- Prepare coordinated release 1.0.0-beta.4.
* Wed Sep 02 2026 Skwd maintainers <noreply@local> - 1.0.0~beta.3-1
- Prepare coordinated release 1.0.0-beta.3.
* Wed Sep 02 2026 Skwd maintainers <noreply@local> - 1.0.0~beta.2-1
- Prepare coordinated release 1.0.0-beta.2.
* Mon Aug 31 2026 Skwd maintainers <noreply@local> - 1.0.0~beta.1-1
- Prepare coordinated release 1.0.0-beta.1.
* Mon Aug 17 2026 Skwd maintainers <noreply@local> - 0.1.0-1
- Establish the optional Steamworks companion package.
