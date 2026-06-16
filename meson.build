project('lushtext',
  version: '0.4.0',
  meson_version: '>= 0.62.0',
  license: 'GPL-3.0-or-later',
)

i18n = import('i18n')
gnome = import('gnome')

# Build profile
profile = get_option('profile')

# Install paths
pkgdatadir = get_option('prefix') / get_option('datadir') / 'lushtext'

# Subdirectories
subdir('resources')
subdir('data')
subdir('po')

# Cargo build via wrapper script
cargo = find_program('build-aux/cargo.sh')
custom_target('cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: 'lushtext',
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  command: [
    cargo,
    meson.project_build_root(),
    meson.project_source_root(),
    '@OUTPUT@',
    profile,
    pkgdatadir,
  ],
)

gnome.post_install(
  glib_compile_schemas: true,
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
