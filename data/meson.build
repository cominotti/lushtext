# Desktop file
desktop_file = i18n.merge_file(
  input: 'dev.cominotti.lushtext.desktop.in',
  output: 'dev.cominotti.lushtext.desktop',
  type: 'desktop',
  po_dir: '../po',
  install: true,
  install_dir: get_option('datadir') / 'applications',
)

desktop_file_validate = find_program('desktop-file-validate', required: false)
if desktop_file_validate.found()
  test('validate-desktop-file',
    desktop_file_validate,
    args: [desktop_file],
  )
endif

# AppStream metainfo
metainfo_file = i18n.merge_file(
  input: 'dev.cominotti.lushtext.metainfo.xml.in',
  output: 'dev.cominotti.lushtext.metainfo.xml',
  po_dir: '../po',
  install: true,
  install_dir: get_option('datadir') / 'metainfo',
)

appstreamcli = find_program('appstreamcli', required: false)
if appstreamcli.found()
  test('validate-metainfo',
    appstreamcli,
    args: ['validate', '--no-net', '--explain', metainfo_file],
  )
endif

# Icons
install_data(
  'icons/dev.cominotti.lushtext.svg',
  install_dir: get_option('datadir') / 'icons' / 'hicolor' / 'scalable' / 'apps',
)
install_data(
  'icons/dev.cominotti.lushtext-symbolic.svg',
  install_dir: get_option('datadir') / 'icons' / 'hicolor' / 'symbolic' / 'apps',
)
install_data(
  'icons/hicolor/32x32/apps/dev.cominotti.lushtext.png',
  install_dir: get_option('datadir') / 'icons' / 'hicolor' / '32x32' / 'apps',
)
install_data(
  'icons/hicolor/64x64/apps/dev.cominotti.lushtext.png',
  install_dir: get_option('datadir') / 'icons' / 'hicolor' / '64x64' / 'apps',
)
install_data(
  'icons/hicolor/128x128/apps/dev.cominotti.lushtext.png',
  install_dir: get_option('datadir') / 'icons' / 'hicolor' / '128x128' / 'apps',
)

# GSettings schema
install_data(
  'dev.cominotti.lushtext.gschema.xml',
  install_dir: get_option('datadir') / 'glib-2.0' / 'schemas',
)
