desktop_conf = configuration_data()
desktop_conf.set('app_id', application_id)
desktop_file = i18n.merge_file(
  input: configure_file(
    input: '@0@.desktop.in'.format(base_id),
    output: '@0@.desktop.in'.format(application_id),
    configuration: desktop_conf,
  ),
  output: '@0@.desktop'.format(application_id),
  type: 'desktop',
  po_dir: '../po',
  install: true,
  install_dir: datadir / 'applications',
)
if desktop_file_validate.found()
  test('Validate desktop file', desktop_file_validate, args: [desktop_file])
endif

appstream_file = i18n.merge_file(
  input: configure_file(
    input: '@0@.metainfo.xml.in'.format(base_id),
    output: '@0@.metainfo.xml.in'.format(application_id),
    configuration: desktop_conf,
  ),
  output: '@0@.metainfo.xml'.format(application_id),
  po_dir: '../po',
  install: true,
  install_dir: datadir / 'metainfo',
)
if appstreamcli.found()
  test('Validate appstream file', appstreamcli,
    args: ['validate', '--no-net', '--explain', appstream_file])
endif

install_data(
  '@0@.gschema.xml'.format(base_id),
  install_dir: datadir / 'glib-2.0' / 'schemas',
)
test('Validate schema file', glib_compile_schemas,
  args: ['--strict', '--dry-run', meson.current_source_dir()])

install_data(
  'icons/hicolor/scalable/apps/@0@.svg'.format(base_id),
  install_dir: iconsdir / 'hicolor' / 'scalable' / 'apps',
  rename: '@0@.svg'.format(application_id),
)
install_data(
  'icons/hicolor/symbolic/apps/@0@-symbolic.svg'.format(base_id),
  install_dir: iconsdir / 'hicolor' / 'symbolic' / 'apps',
  rename: '@0@-symbolic.svg'.format(application_id),
)

# Blueprint, GResource und Schema werden von build.rs kompiliert; Meson
# gibt lediglich die Quellen als Abhängigkeit an das Cargo-Ziel weiter.
data_sources = files(
  'gesetze.gresource.xml',
  'style.css',
  '@0@.gschema.xml'.format(base_id),
)
blueprint_sources = files(
  'ui/window.blp',
  'ui/law_tab.blp',
  'ui/norm_view.blp',
  'ui/outline_row.blp',
  'ui/annotation_row.blp',
  'ui/search_row.blp',
  'ui/preferences.blp',
  'ui/shortcuts.blp',
)
