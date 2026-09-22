cargo_options = ['--manifest-path', meson.project_source_root() / 'Cargo.toml']
cargo_options += ['--target-dir', meson.project_build_root() / 'src']

if get_option('buildtype') == 'release'
  cargo_options += ['--release']
  rust_target = 'release'
else
  rust_target = 'debug'
endif

cargo_env = environment()
cargo_env.set('CARGO_HOME', meson.project_build_root() / 'cargo-home')
cargo_env.set('APP_ID', application_id)
cargo_env.set('VERSION', meson.project_version() + version_suffix)
cargo_env.set('PROFILE', get_option('profile'))
cargo_env.set('LOCALEDIR', localedir)
cargo_env.set('PKGDATADIR', pkgdatadir)
cargo_env.set('GSCHEMA_DIR', datadir / 'glib-2.0' / 'schemas')

rust_sources = files(
  'main.rs',
  'application.rs',
  'config.rs',
  'window.rs',
  'db/mod.rs',
  'importer/mod.rs',
  'model/mod.rs',
  'refs/mod.rs',
  'widgets/mod.rs',
)

cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: bindir,
  depend_files: rust_sources + data_sources + blueprint_sources,
  env: cargo_env,
  command: [
    'sh', '-c',
    '@0@ build @1@ && cp "@2@" "@3@"'.format(
      cargo.full_path(),
      ' '.join(cargo_options),
      meson.project_build_root() / 'src' / rust_target / meson.project_name(),
      '@OUTPUT@',
    ),
  ],
)

test('cargo test', cargo,
  args: ['test', '--manifest-path', meson.project_source_root() / 'Cargo.toml',
         '--target-dir', meson.project_build_root() / 'src'],
  env: cargo_env,
  timeout: 1800,
)
test('cargo clippy', cargo,
  args: ['clippy', '--manifest-path', meson.project_source_root() / 'Cargo.toml',
         '--target-dir', meson.project_build_root() / 'src', '--', '-D', 'warnings'],
  env: cargo_env,
  timeout: 1800,
)
test('cargo fmt', cargo,
  args: ['fmt', '--manifest-path', meson.project_source_root() / 'Cargo.toml', '--check'],
  timeout: 300,
)
