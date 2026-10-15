import type { Catalogue } from './check';

/**
 * The English catalogue: every text the shell shows (D44, docs/translations.md). A translation is
 * a JSON file with the same keys (`<code>.json`); anything it leaves out shows in English.
 *
 * - Keys are grouped by window, e.g. `parts.empty.title`.
 * - `{name}` is a placeholder, filled in by `t('key', { name })`.
 * - A plural is `{ one: '{count} part', other: '{count} parts' }`; `count` picks the form.
 * - `<b>…</b>` and `<code>…</code>` mark bold and code inside a sentence (components/RichText).
 *
 * This file has no imports at run time, so `npm run i18n:check` can read it without a build.
 */
export const en = {
  // ---- Desktop: boot screen, start menu, taskbar, windows, About, and errors the shell makes ----

  // Boot / loading screen
  'boot.connecting': 'Connecting to the core…',
  'boot.waiting': 'Waiting for the Faber core. Is faber-core running?',
  'boot.mismatch.title': 'Restart the Faber core',
  'boot.mismatch.message':
    'This desktop was built for a different version of the Faber core than the one running. Stop faber-core and start it again (build it first if you changed the code). This page connects by itself once the core is back.',
  'boot.mismatch.versions': 'desktop {shell} · core {core}',
  'boot.mismatch.coreTooOld': 'too old to say',

  // Connection indicator (taskbar)
  'connection.connected': 'Connected',
  'connection.connected.detail': 'Live connection to the Faber core.',
  'connection.connecting': 'Connecting…',
  'connection.connecting.detail': 'Opening the connection to the Faber core.',
  'connection.reconnecting': 'Reconnecting…',
  'connection.reconnecting.detail':
    'Lost the connection to the Faber core. Trying again on its own; check that faber-core is running.',
  'connection.closed': 'Not connected',
  'connection.closed.detail': 'The shell is not connected to the Faber core.',

  // Desktop
  'desktop.windowsArea': 'Windows',
  'desktop.toast.dismiss': 'Dismiss',

  // Start menu
  'startMenu.label': 'Start',
  'startMenu.search.placeholder': 'Search programs, parts and files',
  'startMenu.search.label': 'Search Faber',
  'startMenu.search.nothing': 'Nothing called “{query}”.',
  'startMenu.programs': 'Programs',
  'startMenu.parts': 'Parts',
  'startMenu.viewers': 'Viewers',
  'startMenu.recent': 'Recent files',
  'startMenu.recent.empty': 'Files you open show up here.',
  'startMenu.recent.home': 'Home',
  'startMenu.recent.yesterday': 'Yesterday',
  'startMenu.recent.date': '{day}-{month}-{year}',
  'startMenu.openInParts': 'Open in Parts',
  'startMenu.scanWindowTitle': '{part} · scan',
  /** Beside a program in the search results. */
  'startMenu.group.tool': 'Tool',
  'startMenu.group.accessory': 'Accessory',
  'startMenu.group.faber': 'Faber',
  'startMenu.power.label': 'Power options',
  'startMenu.power.menu': 'Power',
  'startMenu.power.restart': 'Restart machine',
  'startMenu.power.shutdown': 'Shut down machine',
  'startMenu.power.unavailable': 'Available on the Faber machine image (Phase 6)',

  // Program menus (start menu, taskbar, desktop icons)
  'menu.open': 'Open',
  'programs.menu.pin': 'Pin to taskbar',
  'programs.menu.unpin': 'Unpin from taskbar',

  // Taskbar
  'taskbar.label': 'Taskbar',
  'taskbar.tasks': 'Programs and windows',
  'taskbar.task.windows': { one: '{name} ({count} window)', other: '{name} ({count} windows)' },
  'taskbar.menu.newWindow': 'New window',
  'taskbar.menu.closeWindow': 'Close window',
  'taskbar.menu.closeAllWindows': 'Close all windows',
  'taskbar.menu.small': 'Small taskbar',
  'taskbar.menu.large': 'Large taskbar, with labels',
  'taskbar.menu.settings': 'Taskbar settings…',

  // Window menus (title bar and taskbar)
  'window.menu.show': 'Show',
  'window.menu.minimise': 'Minimise',
  'window.menu.maximise': 'Maximise',
  'window.menu.restore': 'Restore size',
  'window.menu.close': 'Close',

  // Window frame
  'window.titlebar.label':
    '{title} window. Arrow keys move it, Shift and arrow keys resize it, Enter maximises it.',
  'window.tabs.label': '{title} tabs',
  'window.tab.new': 'New tab',
  'window.tab.close': 'Close tab',
  'window.tab.closeNamed': 'Close tab {title}',
  'window.tab.unsaved': 'Unsaved changes',
  'window.control.minimise': 'Minimise {title}',
  'window.control.maximise': 'Maximise {title}',
  'window.control.restore': 'Restore {title}',
  'window.control.close': 'Close {title}',
  'desktop.label': 'Desktop',
  'desktop.menu.newFolder': 'New folder',
  'desktop.menu.newTextFile': 'New text file',
  'desktop.menu.lineUp': 'Line up icons',
  'desktop.menu.wallpaper': 'Change wallpaper…',
  'desktop.menu.refresh': 'Refresh',
  /** Names given to a new folder or file on the desktop, before the person renames it. */
  'desktop.newFolder.name': 'New folder',
  'desktop.newTextFile.name': 'New text file.txt',
  'desktop.rename.label': 'New name for {name}',
  /** The Faber lockup on the loading screen: "Faber" stays, "OS" may be translated. */
  'desktop.wordmark.os.label': 'Faber OS',
  'desktop.wordmark.os': 'OS',
  'menu.label': 'Menu',

  // About Faber
  'about.tagline':
    'Latin for <b>maker</b>. An open-source operating system for the machines you build.',
  'about.tryAgain': 'Try again',
  'about.core': 'Core',
  'about.core.unknown': 'Not known yet',
  'about.shell': 'Shell',
  'about.address': 'Address',
  'about.connection': 'Connection',
  'about.connection.detail': '{status}, WebSocket as <code>{format}</code>',
  'about.status.connected': 'connected',
  'about.status.connecting': 'connecting',
  'about.status.reconnecting': 'reconnecting',
  'about.status.closed': 'closed',
  'about.licence': 'Apache-2.0. Faber is in early development: this is shell v0.',

  // Programs: window titles and start-menu descriptions (windows/registry.ts)
  'programs.parts.title': 'Parts',
  'programs.parts.description': 'Add, watch and remove the hardware on this machine',
  'programs.laserScan.title': 'Scan viewer',
  'programs.laserScan.description': 'Live top view of a laser-scan signal',
  'programs.serialMonitor.title': 'Serial monitor',
  'programs.serialMonitor.description': 'Read, write and record a serial port directly',
  'programs.logs.title': 'Logs',
  'programs.logs.description': 'Everything the core reports, live',
  'programs.files.title': 'Files',
  'programs.files.description': 'Your desktop, documents, recordings and machine config',
  'programs.notes.title': 'Notes',
  'programs.notes.description': 'A plain text editor',
  'programs.settings.title': 'Settings',
  'programs.settings.description': 'Mode, style, wallpaper, taskbar and developer settings',
  'programs.properties.title': 'Properties',
  'programs.properties.description': 'Details of one file or folder',
  'programs.about.title': 'About Faber',
  'programs.about.description': 'Versions and connection details',

  // Errors the shell makes itself (errors the core sends are shown as sent)
  'errors.coreUnreachable.message': 'The shell could not reach the Faber core.',
  'errors.coreUnreachable.hint':
    'Check that faber-core is running and that this computer can reach the machine, then try again.',
  'errors.badJson.message': "The core's answer to {method} {path} was not valid JSON.",
  'errors.badJson.hint': 'The shell and the core may be different versions. Reload the page.',
  /** `status` is the HTTP status, e.g. "500 Internal Server Error". */
  'errors.badStatus.message': 'The core answered {method} {path} with {status}.',
  'errors.badStatus.hint':
    'Try again. If it keeps happening, look in the Logs tool for what the core reported.',
  'errors.socket.closedByShell': 'The shell closed its connection to the core.',
  'errors.socket.lost': 'The connection to the core was lost.',
  'errors.socket.notConnected.message': 'The shell is not connected to the core right now.',
  'errors.socket.notConnected.hint':
    'Wait until the taskbar says Connected, then open the port again.',
  'errors.wire.unreadable': 'A message from the core could not be read ({reason}).',
  'errors.wire.unknownType': 'The core sent a message of an unknown type.',
  'errors.wire.hint':
    'The shell and the core may be different versions. Reload the page; if it keeps happening, update Faber.',

  // ---- Parts, adding a part, the scan viewer, port problems -------------------------------------

  // Parts window: toolbar and summary
  'parts.loading': 'Loading parts…',
  // The number is drawn in its own span; the window splits the text around it.
  'parts.summary.count': { one: '{count} part', other: '{count} parts' },
  'parts.summary.file': 'from <code>parts.toml</code>',
  'parts.toolbar.add': 'Add part',

  // Parts window: not connected
  'parts.offline.message': 'Not connected to the core, so the states below may be out of date.',
  'parts.offline.hint':
    'Faber reconnects on its own. If this stays, check that faber-core is running.',

  // Parts window: empty state. `{link}` is the button `parts.empty.sample`.
  'parts.empty.title': 'No parts yet',
  'parts.empty.body': 'Plug in your hardware. Faber finds it and suggests what it is.',
  'parts.empty.noHardware': 'No hardware yet? {link}.',
  'parts.empty.sample': 'Try a sample recording',
  'parts.empty.addFirst': 'Add the first part',

  // Parts window: the list
  'parts.list.label': 'Parts',
  'parts.list.openViewer': 'Open the viewer',
  'parts.list.bluetoothPort': 'Bluetooth: {label}',
  'parts.list.port': 'Port',
  'parts.list.confirmRemove': 'Remove from parts.toml?',
  'parts.list.removing': 'Removing…',
  'parts.list.remove': 'Remove',
  'parts.list.keep': 'Keep',
  'parts.list.viewFor': 'Open the viewer for {address}',
  'parts.list.view': 'View',
  'parts.list.removeNamed': 'Remove {name}',

  // Parts window: right-click menu on a part. `{kind}` is a `parts.portKind.*` in lower case.
  'parts.menu.view': 'Open viewer',
  'parts.menu.noSignal': 'This part has no signal Faber can draw yet.',
  'parts.menu.monitorKind': 'Open {kind} port in serial monitor',
  'parts.menu.monitor': 'Open port in serial monitor',
  'parts.menu.remove': 'Remove',
  'parts.menu.busy.self':
    '"{part}" is using this port, and a port can only be open once. Remove the part to talk to the port directly.',
  'parts.menu.busy.other': 'The port is in use by {owner}.',
  'parts.menu.busy.missing': "{port} isn't connected right now.",
  'parts.menu.simulatedNoPort': 'A simulated part has no port.',
  'parts.list.simulated.manual': 'Simulated, values set by hand',
  'parts.list.simulated.world': 'Simulated in the world',

  // Window titles opened from the Parts window
  'parts.title.viewer': '{name} · scan',
  'parts.title.recordings': 'Recordings · Files',

  // What kind of link a port is
  'parts.portKind.usbSerial': 'USB serial',
  'parts.portKind.serial': 'Serial',
  'parts.portKind.bluetooth': 'Bluetooth',
  'parts.portKind.replay': 'Recording',

  // Add part form
  'addPart.form.label': 'Add part',
  'addPart.back': 'Parts',
  'addPart.title': 'Add part',

  // Add part form: step 1, the port. `{tool}` is the tool's id with spaces, e.g. "serial monitor".
  'addPart.port.legend': 'Pick the port it is plugged into',
  'addPart.port.looking': 'Looking for ports…',
  'addPart.port.none':
    'No ports found. Plug in the USB adapter, then wait a moment: this list updates on its own.',
  'addPart.port.bluetooth': 'Bluetooth',
  'addPart.port.oftenUsedFor': 'Often used for: {name}',
  'addPart.port.free': 'Free',
  'addPart.port.usedByTool': 'Used by the {tool}',
  'addPart.port.usedByPart': 'Used by the part "{part}"',

  // Add part form: step 2, the connector
  'addPart.connector.legend': 'Pick the connector that talks to it',
  'addPart.connector.retry': 'Try again',
  'addPart.connector.loading': 'Loading connectors…',
  'addPart.connector.version': 'v{version}',
  'addPart.connector.serial': 'serial, {baud} baud',
  'addPart.connector.bridge': 'needs a bridge (Phase 4); simulated for now',
  'addPart.connector.provides': 'Provides {capabilities}',

  // Add part form: step 3, the name (also the checks in lib/state/partName.ts)
  'addPart.name.legend': 'Give it a name',
  'addPart.name.label': 'Name',
  'addPart.name.placeholder': 'front_lidar',
  'addPart.name.help':
    'Lowercase letters, digits and _, starting with a letter. Apps use it to find the part.',
  'addPart.name.empty': 'The part needs a name.',
  'addPart.name.tooLong': '"{name}" is longer than {max} characters.',
  'addPart.name.invalid': '"{name}" isn\'t a valid part name.',
  'addPart.name.hint':
    'Use lowercase letters, digits and _, starting with a letter, e.g. front_lidar.',

  // Add part form: footer
  'addPart.footer.pickPort': 'Pick a port to continue.',
  'addPart.footer.pickConnector': 'Pick a connector to continue.',
  'addPart.footer.ready': 'Adds the part to <code>parts.toml</code> and starts it.',
  'addPart.footer.cancel': 'Cancel',
  'addPart.footer.adding': 'Adding…',
  'addPart.footer.submit': 'Add part',

  // Laser scan viewer. `{state}` is the core's state id, e.g. "degraded".
  'scan.canvas.label':
    'Top view of {address}, forward is up. Plus and minus zoom, arrow keys pan, 0 resets the view.',
  'scan.canvas.error.message': 'This browser could not start drawing the scan.',
  'scan.canvas.error.hint':
    'Try another browser, or turn on hardware acceleration in the browser settings.',
  'scan.waiting': 'Waiting for the first scan from <code>{address}</code>…',
  'scan.partState': 'The part is {state}.',
  'scan.partStateWithMessage': 'The part is {state}: {message}',
  'scan.zoom.label': 'View',
  'scan.zoom.in': 'Zoom in',
  'scan.zoom.out': 'Zoom out',
  'scan.zoom.fit': 'Fit the scan',
  'scan.stats.label': 'Scan statistics',
  'scan.stats.points': 'Points',
  'scan.stats.noReturn': 'No return',
  'scan.stats.rate': 'Rate',
  'scan.stats.signal': 'Signal',
  'scan.stats.partState': 'Part state',

  // Port problems (USB devices that never became a port); the count shows only above 1
  'portProblems.count': { one: '({count} times)', other: '({count} times)' },
  'portProblems.whatToTry': 'What to try',

  // Part state badge
  'partState.running': 'Running',
  'partState.starting': 'Starting',
  'partState.degraded': 'Degraded',
  'partState.failed': 'Failed',
  'partState.conflict': 'Conflict',
  'partState.invalid': 'Invalid',
  'partState.stopped': 'Stopped',

  // ---- Files, the file picker, Properties -------------------------------------------------------

  // Window and tab titles
  'files.windowTitle.plain': 'Files',
  'files.windowTitle.folder': '{name} · Files',
  'files.windowTitle.notes': '{name} · Notes',
  'files.windowTitle.properties': '{name} · Properties',
  'files.home': 'Home',
  'files.newTab': 'New tab',

  // Toolbar
  'files.toolbar.back': 'Back',
  'files.toolbar.backHint': 'Back (Alt+Left)',
  'files.toolbar.forward': 'Forward',
  'files.toolbar.forwardHint': 'Forward (Alt+Right)',
  'files.toolbar.up': 'Up one folder',
  'files.toolbar.upHint': 'Up one folder (Alt+Up)',
  'files.toolbar.path': 'Folder path',
  'files.toolbar.newFolder': 'New folder',
  'files.toolbar.upload': 'Upload',
  'files.toolbar.showAs': 'Show as',
  'files.toolbar.showAsHint': 'Show as (Ctrl and the wheel, or Ctrl + and Ctrl −)',

  // Views (lib/desktop/fileZoom.ts)
  'files.view.details': 'Details',
  'files.view.small': 'Small icons',
  'files.view.large': 'Large icons',

  // Sidebar
  'files.side.places': 'Places',
  'files.side.disks': 'Disks',

  // Folder pane
  'files.pane.contentsOf': 'Contents of {folder}',
  'files.pane.list': 'Files and folders',
  'files.pane.renameInput': 'New name for {name}',
  'files.pane.dropHere': 'Drop to upload to {folder}',
  'files.pane.loading': 'Loading…',
  'files.explain.trash': 'Files here can be restored. Emptying the Trash deletes them for good.',
  'files.explain.system':
    '<code>parts.toml</code> here is your machine config. Saving it applies it at once; a broken file is refused with the line that is wrong.',
  'files.explain.disk':
    'A removable disk. It has no Trash: deleting a file here deletes it for good.',
  'files.column.name': 'Name',
  'files.column.from': 'From',
  'files.column.size': 'Size',
  'files.column.modified': 'Modified',

  // Empty folders
  'files.empty.recordings.title': 'No recordings yet',
  'files.empty.recordings.body':
    "Record a port with the serial monitor's Record button and it appears here. Every recording is also a replay port, so you can add a part on it as if the hardware were plugged in.",
  'files.empty.recordings.sample':
    'No hardware yet? Faber comes with one sample: a short recording of an LD19 LiDAR in a room. Adding it copies it here as <code>ld19-sample.fbcap</code>.',
  'files.empty.recordings.addSamples': 'Add sample recordings',
  'files.empty.trash': 'The Trash is empty.',
  'files.empty.folder': 'This folder is empty.',
  'files.empty.folderDrop':
    'This folder is empty. Drop files here from your computer to upload them.',

  // Details panel and status line
  'files.details.label': 'Details',
  'files.details.kind': 'Kind',
  'files.details.size': 'Size',
  'files.details.modified': 'Modified',
  'files.details.port': 'Port',
  'files.details.addPart': 'Add a part on it',
  'files.details.open': 'Open',
  'files.details.download': 'Download',
  'files.details.folderSummary': {
    one: '{count} item. Select a file to see its details.',
    other: '{count} items. Select a file to see its details.',
  },
  'files.status.items': { one: '{count} item', other: '{count} items' },
  'files.status.itemsSelected': {
    one: '{count} item · 1 selected',
    other: '{count} items · 1 selected',
  },

  // File types (lib/desktop/fileTypes.ts)
  'files.type.folder': 'Folder',
  'files.type.text': 'Text',
  'files.type.markdown': 'Markdown text',
  'files.type.config': '{ext} settings',
  'files.type.code': '{ext} source code',
  'files.type.recording': 'Recording',
  'files.type.image': '{ext} image',
  'files.type.other': '{ext} file',
  'files.type.file': 'File',

  // Names of new files and folders
  'files.newName.folder': 'New folder',
  'files.newName.textFile': 'New text file.txt',

  // Right-click menus
  'files.menu.header.file': '{size} · {folder}',
  'files.menu.header.folder': 'Folder · {folder}',
  'files.menu.open': 'Open',
  'files.menu.openTab': 'Open in new tab',
  'files.menu.openWithNotes': 'Open with Notes',
  'files.menu.rename': 'Rename',
  'files.menu.download': 'Download',
  'files.menu.properties': 'Properties',
  'files.menu.restore': 'Restore',
  'files.menu.moveToTrash': 'Move to Trash',
  'files.menu.deleteForGood': 'Delete for good',
  'files.menu.deleteForGoodAsk': 'Delete for good…',
  'files.menu.fixedFolder':
    "{name} is one of Faber's fixed folders, so it can't be renamed or moved.",
  'files.menu.newFolder': 'New folder',
  'files.menu.newTextFile': 'New text file',
  'files.menu.uploadFiles': 'Upload files…',
  'files.menu.refresh': 'Refresh',
  'files.menu.emptyTrash': 'Empty Trash',
  'files.menu.trashAlreadyEmpty': 'The Trash is already empty.',

  // Notices and actions on errors
  'files.notice.diskRemoved': 'The disk was removed, so Files went back to your home folder.',
  'files.action.tryAgain': 'Try again',
  'files.action.dismiss': 'Dismiss',
  'files.action.cancel': 'Cancel',

  // Uploads (lib/desktop/uploads.ts) and the name-taken dialog
  'files.upload.renamedPair': '{from} → {to}',
  'files.upload.keptBoth': 'Kept both: {names}.',
  'files.upload.replaced': 'Replaced {names}.',
  'files.upload.skipped': 'Skipped {names}.',
  'files.dialog.conflict.title': '“{name}” is already in {folder}',
  'files.dialog.conflict.body':
    'Replace the file that is there, keep both (the upload gets a new name), or skip it?',
  'files.dialog.conflict.forAll': 'Do the same for the other files that are already there',
  'files.dialog.conflict.skip': 'Skip',
  'files.dialog.conflict.replace': 'Replace',
  'files.dialog.conflict.keepBoth': 'Keep both',

  // Delete for good / empty the Trash
  'files.dialog.deleteForGood.title': 'Delete “{name}” for good?',
  'files.dialog.deleteForGood.body': "It can't be restored afterwards.",
  'files.dialog.deleteForGood.bodyDisk': "Disks have no Trash, so it can't be restored afterwards.",
  'files.dialog.deleteForGood.confirm': 'Delete for good',
  'files.dialog.emptyTrash.title': 'Empty the Trash?',
  'files.dialog.emptyTrash.body': {
    one: "All {count} item in the Trash will be deleted for good. They can't be restored afterwards.",
    other:
      "All {count} items in the Trash will be deleted for good. They can't be restored afterwards.",
  },
  'files.dialog.emptyTrash.confirm': 'Empty Trash',

  // Open and save dialog (components/FilePicker.svelte)
  'filePicker.title.open': 'Open a file',
  'filePicker.title.save': 'Save as',
  'filePicker.places': 'Places',
  'filePicker.up': 'Up one folder',
  'filePicker.list': 'Files and folders in {path}',
  'filePicker.loading': 'Loading…',
  'filePicker.nothingHere': 'Nothing here.',
  'filePicker.fileName': 'File name',
  'filePicker.taken': 'There is already a file called “{name}” here. Saving replaces it.',
  'filePicker.cancel': 'Cancel',
  'filePicker.open': 'Open',
  'filePicker.replace': 'Replace',
  'filePicker.save': 'Save',

  // Properties window
  'properties.recording.intro':
    '<b>This is a recording</b>: bytes captured from a serial port, with the time each arrived. Faber plays every recording in Recordings as a replay port, so you can add a part on it exactly as if the hardware were plugged in.',
  'properties.recording.port': 'Its port is <code>{port}</code>.',
  'properties.recording.addPart': 'Add a part on this recording',
  'properties.kind': 'Kind',
  'properties.kind.fixedFolder': "Faber's fixed folder",
  'properties.kind.file': 'File',
  'properties.kind.folder': 'Folder',
  'properties.size': 'Size',
  'properties.sizeValue': '{size} ({bytes} bytes)',
  'properties.modified': 'Modified',
  'properties.path': 'Path',
  'properties.cameFrom': 'Came from',
  'properties.showInFiles': 'Show in Files',
  'properties.openWithNotes': 'Open with Notes',
  'properties.download': 'Download',

  // ---- Notes, Serial monitor, Logs --------------------------------------------------------------

  // Notes: window and tabs
  'notes.window.title': '{name} · Notes',
  'notes.tabs.add': 'New note',
  'notes.untitled': 'Untitled',
  'notes.untitledNumbered': 'Untitled {n}',

  // Notes: toolbar
  'notes.toolbar.label': 'Notes',
  'notes.toolbar.new': 'New',
  'notes.toolbar.new.title': 'New note in a new tab',
  'notes.toolbar.open': 'Open…',
  'notes.toolbar.open.title': 'Open (Ctrl+O)',
  'notes.toolbar.save': 'Save',
  'notes.toolbar.save.title': 'Save (Ctrl+S)',
  'notes.toolbar.saveAs': 'Save as…',
  'notes.toolbar.saveAs.title': 'Save as (Ctrl+Shift+S)',
  'notes.toolbar.find': 'Find',
  'notes.toolbar.find.title': 'Find (Ctrl+F)',
  'notes.toolbar.wrap': 'Word wrap',
  'notes.toolbar.mono': 'Monospace',
  'notes.toolbar.mono.title': 'Monospace font',
  'notes.toolbar.mono.sample': 'Aa',

  // Notes: find bar
  'notes.find.label': 'Find',
  'notes.find.placeholder': 'Find',
  'notes.find.none': 'No matches',
  'notes.find.position': '{index} of {total}',
  'notes.find.found': { one: '{count} found', other: '{count} found' },
  'notes.find.matchCase': 'Match case',
  'notes.find.previous': 'Previous match',
  'notes.find.previous.title': 'Previous (Shift+Enter)',
  'notes.find.next': 'Next match',
  'notes.find.next.title': 'Next (Enter)',
  'notes.find.close': 'Close find',

  // Notes: editor, errors and status bar
  'notes.dismiss': 'Dismiss',
  'notes.opening': 'Opening <code>{path}</code>…',
  'notes.tryAgain': 'Try again',
  'notes.closeTab': 'Close tab',
  'notes.editor.label': 'Text of {name}',
  'notes.status.position': 'Ln {line}, Col {column}',
  'notes.status.eol.crlf': 'CRLF',
  'notes.status.eol.lf': 'LF',
  'notes.status.encoding': 'UTF-8',
  'notes.status.saving': 'Saving…',
  'notes.status.unsaved': 'Unsaved changes',
  'notes.status.saved': 'Saved',
  'notes.error.tooBig.message': 'This file is bigger than 16 MB, too big for Notes.',
  'notes.error.tooBig.hint': 'Download it from Files and open it on your computer instead.',
  'notes.error.notText.message': "This file isn't text, so Notes can't open it.",
  'notes.error.notText.hint':
    'Download it from Files to open it with a program that understands it.',

  // Notes: unsaved changes dialog
  'notes.dialog.unsaved.title': 'Save changes to {name}?',
  'notes.dialog.unsaved.message': "If you don't save, your changes are lost.",
  'notes.dialog.unsaved.dontSave': "Don't save",
  'notes.dialog.unsaved.cancel': 'Cancel',
  'notes.dialog.unsaved.save': 'Save',

  // Serial monitor: port controls
  'serial.port.label': 'Port',
  'serial.port.none': 'No ports found',
  'serial.port.option': '{label} ({id})',
  'serial.port.optionBluetooth': 'Bluetooth: {label} ({id})',
  'serial.port.optionBusy': '{option} · {reason}',
  'serial.baud.label': 'Baud',
  'serial.close': 'Close port',
  'serial.open': 'Open port',

  // Serial monitor: recording
  'serial.record.button': 'Record',
  'serial.record.label': 'Record',
  'serial.record.name': 'Recording name',
  'serial.record.start': 'Start',
  'serial.record.cancel': 'Cancel',
  'serial.record.help':
    'Saved in Recordings as <code>{name}.fbcap</code>, and usable as a replay port at once.',
  'serial.record.starting': 'Starting the recording…',
  'serial.record.recordingTo': 'Recording to <code>{path}</code> ({size})',
  'serial.record.stop': 'Stop',
  'serial.record.saved': 'Saved <code>{path}</code> ({size}).',
  'serial.record.showInFiles': 'Show in Files',
  'serial.record.dismiss': 'Dismiss',
  'serial.recordingName.required': 'The recording needs a name.',
  'serial.recordingName.tooLong': '"{name}" is longer than {max} characters.',
  'serial.recordingName.invalid': '"{name}" isn\'t a valid recording name.',
  'serial.recordingName.hint':
    'Use lowercase letters, digits and _, starting with a letter, e.g. bench_test.',

  // Serial monitor: state bar and output
  'serial.state.open': '<code>{port}</code> at {baud} baud',
  'serial.state.opening': 'Opening <code>{port}</code>…',
  'serial.state.closedWithReason': 'Closed: {reason}',
  'serial.state.closed': 'Closed',
  'serial.received': '{size} received',
  'serial.mode.label': 'Show as',
  'serial.mode.text': 'Text',
  'serial.mode.hex': 'Hex',
  'serial.clear': 'Clear the output',
  'serial.output.label': 'Received data',
  'serial.output.waiting': 'Waiting for data…',
  'serial.output.closed': 'Open a port to see what it sends.',

  // Serial monitor: send line
  'serial.send.label': 'Line to send',
  'serial.send.placeholder': 'Type a line and press Enter',
  'serial.send.placeholderClosed': 'Open a port to send',
  'serial.send.ending': 'Line ending',
  'serial.send.ending.lf': 'LF',
  'serial.send.ending.crlf': 'CR LF',
  'serial.send.ending.cr': 'CR',
  'serial.send.ending.none': 'No ending',
  'serial.send.button': 'Send',

  // Logs: toolbar
  'logs.levels.label': 'Show at least',
  'logs.levels.error': 'Errors',
  'logs.levels.warn': 'Warnings',
  'logs.levels.info': 'Info',
  'logs.levels.debug': 'Debug',
  'logs.filter.label': 'Filter by text',
  'logs.filter.placeholder': 'Filter by source or text',
  'logs.count': '{shown} of {total}',

  // Logs: list
  'logs.list.label': 'Log',
  'logs.level.error': 'error',
  'logs.level.warn': 'warn',
  'logs.level.info': 'info',
  'logs.level.debug': 'debug',
  'logs.empty.connected': 'Nothing logged yet.',
  'logs.empty.disconnected': 'The log appears once the shell is connected to the core.',
  'logs.empty.noMatch': 'No entries match. Show more levels or clear the filter.',
  'logs.jump': 'Jump to newest',

  // ---- Settings ---------------------------------------------------------------------------------

  // Sidebar
  'settings.nav.label': 'Settings sections',
  'settings.nav.group.you': 'You',
  'settings.nav.group.machine': 'This machine',
  'settings.nav.group.advanced': 'Advanced',
  'settings.nav.appearance': 'Appearance',
  'settings.nav.accessibility': 'Accessibility',
  'settings.nav.units': 'Units',
  'settings.nav.desktop': 'Desktop and taskbar',
  'settings.nav.machine': 'Name',
  'settings.nav.time': 'Date and time',
  'settings.nav.storage': 'Storage and backups',
  'settings.nav.about': 'About and diagnostics',
  'settings.nav.developer': 'Developer',
  'settings.loading': 'Loading settings…',

  // Appearance
  'settings.appearance.title': 'Appearance',
  'settings.appearance.mode.name': 'Mode',
  'settings.appearance.mode.desc':
    'Follow this screen or browser, or pick one. Applies to every screen.',
  'settings.appearance.mode.system': 'System',
  'settings.appearance.mode.light': 'Light',
  'settings.appearance.mode.dark': 'Dark',
  'settings.appearance.style.name': 'Style',
  'settings.appearance.style.desc':
    'Every style has a light and a dark version; the mode above picks which.',
  'settings.appearance.wallpaper.name': 'Wallpaper',
  'settings.appearance.language.name': 'Language',
  'settings.appearance.language.desc':
    'For every screen. Texts a translation is missing show in English.',
  'settings.appearance.language.english': 'English',
  'settings.appearance.language.option': '{name} ({percent} %)',
  'settings.appearance.language.broken': "A translation can't be used: {problem}",

  // Built-in styles and wallpapers (lib/desktop/styles.ts, wallpapers.ts)
  'settings.style.aluminium.label': 'Aluminium',
  'settings.style.aluminium.description': 'Brushed metal with an anodised teal, the default',
  'settings.style.cobalt.label': 'Cobalt',
  'settings.style.cobalt.description': 'Cool greys and a strong blue',
  'settings.style.workshop.label': 'Workshop',
  'settings.style.workshop.description': 'Warm greys and a green like a cutting mat',
  'settings.style.blueprint.label': 'Blueprint',
  'settings.style.blueprint.description': 'Blue paper and navy ink',
  'settings.style.graphite.label': 'Graphite',
  'settings.style.graphite.description': 'Pure greys, no colour at all',
  'settings.style.signal.label': 'Signal',
  'settings.style.signal.description': 'Neutral greys and a signal red',
  'settings.wallpaper.aluminium.label': 'Aluminium',
  'settings.wallpaper.aluminium.description': 'Brushed metal, the default',
  'settings.wallpaper.anodised.label': 'Anodised',
  'settings.wallpaper.anodised.description': 'Deep teal, like an anodised part',
  'settings.wallpaper.cutting-mat.label': 'Cutting mat',
  'settings.wallpaper.cutting-mat.description': 'A self-healing mat with its grid',
  'settings.wallpaper.graphite.label': 'Graphite',
  'settings.wallpaper.graphite.description': 'Plain dark grey, nothing to look at',

  // Accessibility
  'settings.accessibility.title': 'Accessibility',
  'settings.accessibility.intro':
    'These apply to every screen, the attached one too. System follows what this screen or browser asks for.',
  'settings.accessibility.follow.system': 'System',
  'settings.accessibility.follow.on': 'On',
  'settings.accessibility.follow.off': 'Off',
  'settings.accessibility.motion.name': 'Reduce motion',
  'settings.accessibility.motion.desc': 'No window animations or sliding.',
  'settings.accessibility.contrast.name': 'Higher contrast',
  'settings.accessibility.contrast.desc':
    'Stronger text, lines and focus rings, for small screens and bright rooms.',
  'settings.accessibility.textSize.name': 'Text size',
  'settings.accessibility.textSize.desc':
    'Larger text everywhere in Faber, e.g. for the 7" touch screen.',
  'settings.accessibility.textSize.option': '{size} %',

  // Units
  'settings.units.title': 'Units',
  'settings.units.intro': 'How Faber shows measurements, e.g. the rings in the laser-scan viewer.',
  'settings.units.length.name': 'Lengths',
  'settings.units.length.desc': 'Metres and centimetres, or feet and inches.',
  'settings.units.length.metric': 'Metric (m, cm)',
  'settings.units.length.imperial': 'Imperial (ft, in)',

  // Desktop and taskbar
  'settings.desktop.title': 'Desktop and taskbar',
  'settings.desktop.size.name': 'Taskbar size',
  'settings.desktop.size.desc': 'Large has bigger buttons with names, for the 7" touch screen.',
  'settings.desktop.size.small': 'Small',
  'settings.desktop.size.large': 'Large',
  'settings.desktop.pinned.name': 'Pinned to the taskbar',
  'settings.desktop.pinned.desc':
    'Pinned programs are always on the taskbar, in the order you pinned them. You can also right-click a program anywhere to pin or unpin it.',
  'settings.desktop.pinned.pin': 'Pin {program} to the taskbar',

  // Name
  'settings.machine.title': 'Name',
  'settings.machine.name.label': 'Machine name',
  'settings.machine.name.desc':
    'Shown in the browser tab, in About and in backup names. On the Pi it is also the network address: <code>{address}</code>.',
  'settings.machine.problem.empty': 'Give the machine a name.',
  'settings.machine.problem.tooLong': 'At most 63 characters.',
  'settings.machine.problem.characters': 'Only lowercase letters, digits and dashes.',
  'settings.machine.problem.dash': "It can't start or end with a dash.",

  // Date and time
  'settings.time.title': 'Date and time',
  'settings.time.intro':
    "Clocks, file dates and log times show this machine's time in every browser, so a machine in another room or country shows its own time.",
  'settings.time.zone.name': 'Time zone',
  'settings.time.zone.now': 'Now {time}, {date}.',
  'settings.time.zone.machine': "Machine's own ({zone})",
  'settings.time.zone.unknown': 'not known yet',
  'settings.time.clock.name': 'Clock',
  'settings.time.clock.desc': 'For the taskbar clock, file dates and log times.',
  'settings.time.clock.24h': '24-hour (14:30)',
  'settings.time.clock.12h': '12-hour (2:30 pm)',

  // Storage and backups
  'settings.storage.title': 'Storage and backups',
  'settings.storage.disk.name': 'Disk space',
  'settings.storage.disk.label': 'Disk space used',
  'settings.storage.disk.valueText': '{percent} % used',
  'settings.storage.disk.measuring': 'Measuring…',
  'settings.storage.segment.recordings': 'Recordings',
  'settings.storage.segment.documents': 'Documents',
  'settings.storage.segment.desktop': 'Desktop',
  'settings.storage.segment.backups': 'Backups',
  'settings.storage.segment.other': 'Everything else',
  'settings.storage.segment.free': 'Free',
  'settings.storage.full.message': 'The disk is {percent} % full.',
  'settings.storage.full.hint':
    "Delete recordings or old backups you don't need, then empty the Trash. Faber never deletes anything by itself.",
  'settings.storage.backup.name': 'Back up this machine',
  'settings.storage.backup.desc':
    'Saves parts, settings, Desktop and Documents as one .zip in Backups. Download it to keep a copy off this machine.',
  'settings.storage.backup.recordings': 'Include recordings',
  'settings.storage.backup.recordingsSize': 'Include recordings ({size})',
  'settings.storage.backup.now': 'Back up now',
  'settings.storage.backup.busy': 'Backing up…',
  'settings.storage.backup.saved': 'Saved {name} ({size}).',
  'settings.storage.backups.name': 'Backups',
  'settings.storage.backups.open': 'Open in Files',
  'settings.storage.backups.loading': 'Loading…',
  'settings.storage.backups.empty': 'No backups yet.',
  'settings.storage.backups.download': 'Download',
  'settings.storage.backups.restore': 'Restore…',
  'settings.storage.backups.restoreLabel': 'Restore {name}',
  'settings.storage.backups.restoring': 'Restoring…',
  'settings.storage.restore.title': 'Restore this backup?',
  'settings.storage.restore.body':
    "Parts and settings become the ones in the backup, and its files are put back. Files it replaces go to the Trash, so nothing is lost; files that aren't in the backup stay. Whole-machine file access isn't changed.",
  'settings.storage.restore.cancel': 'Cancel',
  'settings.storage.restore.confirm': 'Restore',
  'settings.storage.restore.done': 'Restored {name}. Any files it replaced are in the Trash.',

  // About and diagnostics
  'settings.about.title': 'About and diagnostics',
  'settings.about.version': 'Version',
  'settings.about.api': 'API',
  'settings.about.dataFolder': 'Data folder',
  'settings.about.uptime': 'Running for',
  'settings.about.uptime.hours': '{hours} h {minutes} min',
  'settings.about.uptime.minutes': '{minutes} min {seconds} s',
  'settings.about.os': 'Operating system',
  'settings.about.timeZone': 'Time zone',
  'settings.about.diagnostics.name': 'Diagnostics',
  'settings.about.diagnostics.desc':
    'For a bug report: version, operating system, ports, part states and the last 200 log lines. No names, folders or personal data.',
  'settings.about.diagnostics.copied': 'Copied.',
  'settings.about.diagnostics.copy': 'Copy diagnostics',
  'settings.about.diagnostics.download': 'Download logs',
  'settings.about.diagnostics.manualTitle': 'Copy diagnostics',
  'settings.about.diagnostics.manual':
    "This browser won't let Faber use the clipboard here. Select the text and copy it.",
  'settings.about.diagnostics.done': 'Done',
  'settings.about.reset.name': 'Reset settings to defaults',
  'settings.about.reset.desc':
    'Every setting goes back to how a new machine has it. Parts and files stay.',
  'settings.about.reset.open': 'Reset settings…',
  'settings.about.reset.title': 'Reset every setting?',
  'settings.about.reset.body':
    'Appearance, accessibility, units, date and time, the taskbar and where desktop icons sit go back to how a new machine has them, and whole-machine file access is turned off.',
  'settings.about.reset.untouched': 'Parts and files are not touched.',
  'settings.about.reset.cancel': 'Cancel',
  'settings.about.reset.confirm': 'Reset settings',

  // Developer
  'settings.developer.title': 'Developer',
  'settings.developer.intro':
    'For people building Faber or connectors. Leave these off unless you know why you need them.',
  'settings.developer.machine.name': 'Allow Files to see the whole machine',
  'settings.developer.machine.on':
    'On. Files shows “This computer” with every drive and system folder.',
  'settings.developer.machine.off':
    "Off. Files only shows Faber's home folder and disks. Deleting system files can break this computer, and there is no Trash outside Faber's home folder.",
  'settings.developer.warning.title': 'Let Files see the whole machine?',
  'settings.developer.warning.intro':
    "Files will show every folder on this computer, not just Faber's home folder. Read this first:",
  'settings.developer.warning.system':
    'Deleting or changing system files can stop this computer from starting, and Faber with it.',
  'settings.developer.warning.noTrash':
    'There is <b>no Trash outside the home folder</b>: anything you delete there is gone for good, at once.',
  'settings.developer.warning.others':
    'Anyone using this shell can then read and change those files too.',
  'settings.developer.warning.understood':
    'I understand that I can break this computer and lose files for good.',
  'settings.developer.warning.type': 'Type <code>{word}</code> to confirm',
  // The word people type to confirm: translated, so they type it in their own language.
  'settings.developer.warning.word': 'machine',
  'settings.developer.warning.cancel': 'Cancel',
  'settings.developer.warning.confirm': 'Allow whole-machine access',
} as const satisfies Catalogue;
