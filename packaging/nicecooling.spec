# Sources, made from the repository root:
#   Source0: the repository tree, prefix nicecooling-%{version}/ (packaging/container/build-rpm.sh makes it)
#   Source1: the vendored crates (packaging/vendor.sh makes it)
# The build runs offline: cargo reads only the vendored crates.
%global debug_package %{nil}

Name:           nicecooling
Version:        0.1.0
Release:        1%{?dist}
Summary:        Fan, pump and Leakshield control for a water-cooled desktop
License:        GPL-3.0-or-later
URL:            https://github.com/CappyT/nicecooling
Source0:        %{name}-%{version}.tar.zst
Source1:        %{name}-%{version}-vendor.tar.zst

ExclusiveArch:  x86_64

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  gcc-c++
BuildRequires:  clang
BuildRequires:  cmake
BuildRequires:  pkgconfig
BuildRequires:  zstd
BuildRequires:  systemd-rpm-macros
BuildRequires:  pkgconfig(egl)
BuildRequires:  pkgconfig(fontconfig)
BuildRequires:  pkgconfig(freetype2)
BuildRequires:  pkgconfig(vulkan)
BuildRequires:  pkgconfig(wayland-client)
BuildRequires:  pkgconfig(wayland-cursor)
BuildRequires:  pkgconfig(wayland-egl)
BuildRequires:  pkgconfig(x11)
BuildRequires:  pkgconfig(x11-xcb)
BuildRequires:  pkgconfig(xcb)
BuildRequires:  pkgconfig(xkbcommon)
BuildRequires:  pkgconfig(xkbcommon-x11)

Requires:       dbus-common
Requires:       hicolor-icon-theme
Requires:       polkit
# The app loads these libraries at run time, so rpm does not find them.
Requires:       fontconfig
Requires:       libwayland-client.so.0()(64bit)
Requires:       libwayland-cursor.so.0()(64bit)
Requires:       libwayland-egl.so.1()(64bit)
Requires:       mesa-libEGL
Requires:       vulkan-loader
%{?systemd_requires}

%description
nicecooling controls the fans and the pump of a water-cooled desktop through
the hardware monitoring interface of the Linux kernel, and it reads and
writes the Leakshield. The package holds the daemon nicecoolingd, which runs
as a system service, and the nicecooling app, which shows a tray icon and a
window.

%prep
%autosetup -n %{name}-%{version} -a 1
mkdir -p .cargo
cat > .cargo/config.toml <<'CONFIG'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
CONFIG

%build
export CARGO_HOME="$PWD/.cargo-home"
cargo build --release --offline --locked %{?_smp_mflags} -p nicecoolingd -p nicecooling

%install
install -D -s -m 0755 target/release/nicecooling %{buildroot}%{_bindir}/nicecooling
install -D -s -m 0755 target/release/nicecoolingd %{buildroot}%{_libexecdir}/nicecoolingd
install -D -m 0644 packaging/nicecoolingd.service %{buildroot}%{_unitdir}/nicecoolingd.service
install -D -m 0644 packaging/80-nicecooling.preset %{buildroot}%{_presetdir}/80-nicecooling.preset
install -D -m 0644 packaging/poweroff.target.d/50-nicecooling.conf \
    %{buildroot}%{_unitdir}/poweroff.target.d/50-nicecooling.conf
install -D -m 0644 packaging/dbus/io.github.cappyt.NiceCooling1.conf \
    %{buildroot}%{_datadir}/dbus-1/system.d/io.github.cappyt.NiceCooling1.conf
install -D -m 0644 packaging/polkit/io.github.cappyt.NiceCooling1.policy \
    %{buildroot}%{_datadir}/polkit-1/actions/io.github.cappyt.NiceCooling1.policy
install -D -m 0644 packaging/polkit/50-nicecooling.rules \
    %{buildroot}%{_datadir}/polkit-1/rules.d/50-nicecooling.rules
install -D -m 0644 packaging/io.github.cappyt.NiceCooling.autostart.desktop \
    %{buildroot}%{_sysconfdir}/xdg/autostart/io.github.cappyt.NiceCooling.desktop
install -D -m 0644 packaging/io.github.cappyt.NiceCooling.desktop \
    %{buildroot}%{_datadir}/applications/io.github.cappyt.NiceCooling.desktop
install -D -m 0644 docs/ui/icons/nicecooling.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/io.github.cappyt.NiceCooling.svg
install -d %{buildroot}%{_datadir}/icons/hicolor/symbolic/apps
install -m 0644 docs/ui/icons/nicecooling-*symbolic.svg %{buildroot}%{_datadir}/icons/hicolor/symbolic/apps/
install -d %{buildroot}%{_datadir}/nicecooling/boards %{buildroot}%{_datadir}/nicecooling/calibration
install -m 0644 data/boards/*.toml %{buildroot}%{_datadir}/nicecooling/boards/
install -m 0644 data/calibration/*.toml %{buildroot}%{_datadir}/nicecooling/calibration/
# The daemon writes its state here. The package owns the directory and none of its files.
install -d -m 0700 %{buildroot}%{_sharedstatedir}/nicecooling

%post
%systemd_post nicecoolingd.service

%preun
%systemd_preun nicecoolingd.service

%postun
%systemd_postun_with_restart nicecoolingd.service

%files
%license LICENSE
%doc README.md
%{_bindir}/nicecooling
%{_libexecdir}/nicecoolingd
%{_unitdir}/nicecoolingd.service
%{_presetdir}/80-nicecooling.preset
%dir %{_unitdir}/poweroff.target.d
%{_unitdir}/poweroff.target.d/50-nicecooling.conf
%{_datadir}/dbus-1/system.d/io.github.cappyt.NiceCooling1.conf
%{_datadir}/polkit-1/actions/io.github.cappyt.NiceCooling1.policy
%{_datadir}/polkit-1/rules.d/50-nicecooling.rules
%config(noreplace) %{_sysconfdir}/xdg/autostart/io.github.cappyt.NiceCooling.desktop
%{_datadir}/applications/io.github.cappyt.NiceCooling.desktop
%{_datadir}/icons/hicolor/scalable/apps/io.github.cappyt.NiceCooling.svg
%{_datadir}/icons/hicolor/symbolic/apps/nicecooling-*symbolic.svg
%{_datadir}/nicecooling
%dir %attr(0700,root,root) %{_sharedstatedir}/nicecooling

%changelog
* Mon Nov 23 2026 NiceCooling <nicecooling@localhost.invalid> - 0.1.0-1
- First package
