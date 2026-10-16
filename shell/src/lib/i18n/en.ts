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
  // Notices from the core, shown as toasts (docs/api.md, "Notices")
  'notice.battery.low': 'The battery of {part} is low ({volts}).',
  'notice.battery.low.hint': 'Finish what you are doing and charge it soon.',
  'notice.battery.critical': 'The battery of {part} is critical ({volts}).',
  'notice.battery.critical.hint': 'Stop and charge it now: going on can damage the cells.',
  'notice.battery.ok': 'The battery of {part} is ok again ({volts}).',
  'notice.region': 'Notices',

  // Start menu
  'startMenu.label': 'Start',
  'startMenu.search.placeholder': 'Search programs, apps, parts and files',
  'startMenu.search.label': 'Search Faber',
  'startMenu.search.nothing': 'Nothing called “{query}”.',
  'startMenu.programs': 'Programs',
  'startMenu.parts': 'Parts',
  'startMenu.viewers': 'Viewers',
  'startMenu.apps': 'Apps',
  'startMenu.app.desc': 'App, version {version}',
  'startMenu.group.app': 'App',
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
  'programs.distance.title': 'Distance viewer',
  'programs.distance.description': 'Live distance of a sensor, with a bar and a plot',
  'programs.planarBase.title': 'Drive base viewer',
  'programs.planarBase.description': 'Live velocity of a drive base, commanded and measured',
  'programs.drive.title': 'Drive',
  'programs.drive.description': 'Drive a base or a motor with the pad or the keys',
  'programs.world.title': 'World',
  'programs.world.description': 'See the simulated machine drive around its world',
  'programs.serialMonitor.title': 'Serial monitor',
  'programs.serialMonitor.description': 'Read, write and record a serial port directly',
  'programs.flasher.title': 'Arduino flasher',
  'programs.flasher.description': 'Put a program on an Arduino Uno, Nano or Mega',
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
  'programs.apps.title': 'Apps',
  'programs.apps.description': 'Turn apps on and off, pick their parts and open them',
  'programs.app.title': 'App',
  'programs.app.description': 'An app’s own window',
  'programs.install.title': 'Install connector',
  'programs.install.description': 'Install a connector package (.faberpkg)',
  'programs.position.title': 'Position viewer',
  'programs.position.description': 'Live position of a GPS: coordinates, fix and satellites',
  'programs.battery.title': 'Battery viewer',
  'programs.battery.description':
    'Live voltage of a battery, how full it is, and whether it needs charging',

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
  'parts.list.simulatedBadge': 'Simulated',
  'parts.list.drivenBy': 'Driven by {holder}',
  'parts.list.notDriven': 'Nobody is driving it',
  'parts.list.drive': 'Drive',
  'parts.list.driveNamed': 'Drive {name}',
  'parts.menu.drive': 'Drive',

  // Parts window: a manual simulated sensor's value (docs/api.md, "Manual simulated sensors")
  'parts.manual.value': 'Value of {name}',
  'parts.manual.pattern': 'Pattern of {name}',
  'parts.manual.pattern.steady': 'Steady',
  'parts.manual.pattern.sine': 'Wave',
  'parts.manual.pattern.noise': 'Noise',
  'parts.manual.pattern.step': 'Steps',
  'parts.manual.pattern.drain': 'Drain',

  // Window titles opened from the Parts window
  'parts.title.viewer': '{name} · scan',
  'parts.title.distance': '{name} · distance',
  'parts.title.base': '{name} · base',
  'parts.title.position': '{name} · position',
  'parts.title.battery': '{name} · battery',
  'parts.title.recordings': 'Recordings · Files',

  // What kind of link a port is
  'parts.portKind.usbSerial': 'USB serial',
  'parts.portKind.serial': 'Serial',
  'parts.portKind.bluetooth': 'Bluetooth',
  'parts.portKind.replay': 'Recording',

  // Parts: a FaberLink board (a bridge) and the parts on it (sub-parts, `mega.left`).
  // `{bridge}` is the board's part name, e.g. "mega".
  'parts.list.onBoard': 'On the board {bridge}',
  'parts.list.subParts': 'Parts on {bridge}',
  'parts.board.link.connecting': 'Saying hello to the board…',
  'parts.board.link.describing': 'Asking the board for its parts…',
  'parts.board.link.ready': 'Connected',
  'parts.board.link.noAnswer': 'No answer from the board',
  'parts.board.link.lost': 'Board not reachable',
  'parts.board.link.incompatible': 'Board speaks another FaberLink version',
  'parts.board.firmware': '{board}, firmware {firmware} {version}',
  'parts.board.protocol': 'FaberLink {version}',
  'parts.board.hardwareEstop': 'Stop button pressed',
  'parts.board.hardwareEstop.hint': 'Reset it on the board, then release the e-stop.',

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
  // Checking a port for a FaberLink board (detect, then suggest; only when asked).
  'addPart.port.faberlinkBoard': 'A FaberLink board answered here',
  'addPart.port.check': 'Check for a FaberLink board',
  'addPart.port.checking': 'Checking…',
  'addPart.port.check.help':
    'Opening a port restarts most Arduino boards, so Faber only checks a port when you ask.',
  'addPart.port.check.found': 'FaberLink board: {board} ({firmware} {version})',
  'addPart.port.check.nothing': 'No FaberLink board answered on this port.',
  'addPart.port.check.otherVersion': 'A FaberLink board answered, but it speaks another version.',

  // Add part form: step 2, the connector
  'addPart.connector.legend': 'Pick the connector that talks to it',
  'addPart.connector.retry': 'Try again',
  'addPart.connector.loading': 'Loading connectors…',
  'addPart.connector.version': 'v{version}',
  'addPart.connector.serial': 'serial, {baud} baud',
  'addPart.connector.onBoard': 'real ones sit on a FaberLink board; simulated here',
  'addPart.connector.boardParts': 'Its motors and sensors appear as parts by themselves',
  'addPart.connector.provides': 'Provides {capabilities}',
  // Installed connectors that couldn't be loaded (GET /api/v0/connectors/problems)
  'addPart.connector.problems': {
    one: 'One installed connector could not be loaded, so it is not in this list.',
    other: '{count} installed connectors could not be loaded, so they are not in this list.',
  },
  'addPart.connector.problems.hint': 'Settings → Connectors says why, and what to do.',
  'addPart.connector.problems.open': 'Open Settings',

  // Add part form: simulated parts (D36). `{connector}` is the connector's name.
  'addPart.source.simulated': 'Simulated',
  'addPart.source.simulated.help':
    'No hardware needed. The part gets the same name and capability as the real one, so switching to the real part later is one line in parts.toml.',
  'addPart.source.lockedOnBoard':
    'A real {connector} sits on a FaberLink board: add the board on its port and the part appears by itself. Added here, it can only be simulated.',
  'addPart.source.board':
    'A FaberLink board is real hardware: pick the port it is plugged into. Simulate its parts with the generic connectors instead.',
  'addPart.source.mode': 'Where its values come from',
  'addPart.source.manual': 'Manual',
  'addPart.source.manual.help': 'You set its value in Parts, by hand or as a pattern.',
  'addPart.source.world': 'In the world',
  'addPart.source.world.help':
    'The simulated world gives its values. A drive base becomes the machine in the world.',

  // Add part form: a connector's settings (lib/state/connectorSettings.ts). The labels and
  // descriptions of the settings themselves come from the connector, in English (translations.md).
  'addPart.settings.legend': 'Set its limits',
  'addPart.settings.help':
    'Faber keeps every command within these. They are saved in parts.toml, where you can change them later.',
  'addPart.settings.notANumber': '{label} must be a number.',
  'addPart.settings.mustBeAbove': '{label} must be more than {min}.',
  'addPart.settings.mustBeAtLeast': "{label} can't be below {min}.",
  'addPart.settings.mustBeAtMost': "{label} can't be above {max}.",
  'addPart.settings.advanced': 'Advanced settings',
  'addPart.settings.noParts': 'There are no other parts to pick yet.',

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

  // ---- Driving: the Drive window, the e-stop, simulated parts ----------------------------------

  // Why a lease ended (lib/state/driving.ts)
  'drive.ended.released': 'You stopped driving {part}.',
  'drive.ended.expired':
    'The connection was too slow, so {part} stopped to be safe. Drive again to carry on.',
  'drive.ended.estop': 'Stopped by the e-stop.',
  'drive.ended.takenOver': 'Someone else took over {part}.',
  'drive.ended.partRemoved': '{part} was removed.',
  'drive.ended.partRestarted':
    '{part} restarted because its settings changed. Drive again once it is running.',
  'drive.ended.partFailed': '{part} stopped working. Parts says what went wrong.',
  'drive.ended.shutdown': 'The Faber core is shutting down, so everything stopped.',
  'drive.ended.unknown': 'Driving {part} ended.',

  // Drive window
  'drive.holder': 'Drive window',
  'drive.pick.label': 'Part to drive',
  'drive.pick.none.title': 'Nothing to drive yet',
  'drive.pick.none.body':
    'Add a drive base or a motor in Parts. Tick Simulated to drive one without any hardware.',
  'drive.pick.openParts': 'Open Parts',
  'drive.confirm.title': 'Start driving {machine}?',
  'drive.confirm.body':
    '{part} moves while you use the pad or the keys. Space stops it; the red Stop button or the Pause/Break key stops everything. Faber asks once until this page is reloaded.',
  'drive.confirm.start': 'Start',
  'drive.confirm.cancel': 'Cancel',
  'drive.notStarted': 'Not driving.',
  'drive.start': 'Start driving',
  'drive.status.ready': 'Ready: use the pad or the keys to drive {part}.',
  'drive.status.driving': "You're driving {part}",
  'drive.status.requesting': 'Asking to drive {part}…',
  'drive.status.drivenBy': 'Driven by {holder}',
  'drive.status.estopHint': 'Release the e-stop on the taskbar when it is safe.',
  'drive.takeOver': 'Take over',
  'drive.pad.label': 'Drive pad',
  'drive.pad.help': 'Drag to drive. Let go to stop.',
  'drive.turn.label': 'Turn',
  'drive.speed.label': 'Speed limit',
  'drive.speed.value': '{percent} %',
  'drive.stop': 'Stop',
  'drive.stop.title': 'Engage the e-stop: everything that moves stops',
  'drive.keys': '<b>W A S D</b> or the arrows drive, <b>Q E</b> turn, <b>Space</b> stops.',
  'drive.keys.noSlide':
    '<b>W S</b> or up and down drive, <b>A D</b> or <b>Q E</b> turn, <b>Space</b> stops.',
  'drive.keys.motor': '<b>W S</b> or up and down drive the motor, <b>Space</b> stops.',

  // Velocity gauges (Drive window and the drive base viewer)
  'twist.label': 'Velocity',
  'twist.forward': 'Forward',
  'twist.sideways': 'Sideways',
  'twist.turn': 'Turn',
  'twist.motor': 'Speed',
  'twist.commanded': 'Commanded',
  'twist.measured': 'Measured',
  'twist.bumped': 'Bumped into something',

  // World window (windows/WorldWindow.svelte): the simulated world from above
  'world.canvas.label': 'The world {world} from above',
  'world.loading': 'Loading the world…',
  'world.picker.label': 'World',
  'world.picker.button': 'World: {world}',
  'world.picker.size': '{width} × {height}',
  'world.switch.title': 'Switch to {world}?',
  'world.switch.body': 'The machine goes back to the start.',
  'world.switch.confirm': 'Switch',
  'world.switch.cancel': 'Cancel',
  'world.reset': 'Reset position',
  'world.reset.title': 'Put the machine back at the start, standing still',
  'world.follow': 'Follow the machine',
  'world.drive': 'Drive',
  'world.drive.title': 'Open Drive for {part}',
  'world.zoom.label': 'View',
  'world.zoom.in': 'Zoom in',
  'world.zoom.out': 'Zoom out',
  'world.zoom.fit': 'Fit the world',
  'world.bumped': 'Bumped',
  'world.bumped.detail': 'It stopped short: something is in the way.',
  'world.position': 'Position',
  'world.position.value': '{x}, {y}',
  'world.speed': 'Speed',
  'world.noBase': 'No base: sensors look from the start.',
  'world.empty.title': 'No simulated machine yet',
  'world.empty.body':
    'Add a drive base in Parts with <b>Simulated</b> on and <b>In the world</b> picked. It becomes the machine here; LiDARs and distance sensors added the same way see this world.',
  'world.empty.add': 'Add a simulated base',
  'world.canvas.error.message': "This browser can't draw the world.",
  'world.canvas.error.hint': 'Try a current Chrome, Edge or Firefox.',

  // Drive base viewer
  'base.waiting': 'Waiting for the first reading from <code>{address}</code>…',
  'base.drivenBy': 'Driven by {holder}',
  'base.free': 'Nobody is driving it.',
  'base.drive': 'Drive',

  // Distance viewer
  'distance.waiting': 'Waiting for the first reading from <code>{address}</code>…',
  'distance.status.ok': 'Measuring',
  'distance.status.tooClose': 'Too close: nearer than it can measure',
  'distance.status.tooFar': 'Too far: nothing within its range',
  'distance.status.noTarget': 'No target',
  'distance.status.error': 'The sensor reports an error',
  'distance.bar.label': 'Distance',
  'distance.plot.label': 'The last {seconds} seconds',
  'distance.range': 'Range {min} to {max}',

  // Battery viewer (battery@1)
  'battery.waiting': 'Waiting for the first reading from <code>{address}</code>…',
  'battery.level.ok': 'OK',
  'battery.level.low': 'Low: charge it soon',
  'battery.level.critical': 'Critical: stop and charge it now',
  'battery.volts': 'Voltage',
  'battery.charge': 'Charge',
  'battery.charge.value': '{percent} %',
  'battery.charge.label': 'How full it is',
  'battery.current': 'Current',
  'battery.current.discharging': '{amps} out',
  'battery.current.charging': '{amps} in',
  'battery.temperature': 'Temperature',
  'battery.cells': 'Cells',
  'battery.cell': 'Cell {index}',
  'battery.kind': 'Battery',
  'battery.kind.value': '{chemistry}, {cells} in series',
  'battery.kind.capacity': '{chemistry}, {cells} in series, {capacity} Ah',
  'battery.thresholds': 'Low below {low}, critical below {critical}',
  'battery.chemistry.li-ion': 'Li-ion',
  'battery.chemistry.lipo': 'LiPo',
  'battery.chemistry.lifepo4': 'LiFePO4',
  'battery.chemistry.nimh': 'NiMH',
  'battery.chemistry.lead-acid': 'Lead-acid',
  // Position viewer (position@1, e.g. a GPS)
  'position.waiting': 'Waiting for the first reading from <code>{address}</code>…',
  'position.noFix': 'No fix yet',
  'position.noFix.hint':
    'A GPS needs a clear view of the sky. The first fix can take a minute or more after it is switched on.',
  'position.latitude': 'Latitude',
  'position.longitude': 'Longitude',
  'position.altitude': 'Altitude',
  'position.accuracy': 'Accuracy',
  'position.fix': 'Fix',
  'position.satellites': 'Satellites',
  'position.fix.none': 'None',
  'position.fix.fix2d': '2D (no altitude)',
  'position.fix.fix3d': '3D',
  'position.fix.dgps': '3D, corrected (DGPS)',
  'position.fix.rtkFloat': 'RTK float',
  'position.fix.rtkFixed': 'RTK fixed',
  'position.north': '{deg}° N',
  'position.south': '{deg}° S',
  'position.east': '{deg}° E',
  'position.west': '{deg}° W',
  'position.accuracyValue': '± {value}',
  'position.openMap': 'Open in OpenStreetMap',
  'position.offline': 'Connect this computer to the internet to see it on a map.',

  // Install dialog: a connector package (.faberpkg, docs/packages.md)
  'install.windowTitle': 'Install {name}',
  'install.loading': 'Reading the package…',
  'install.noFile': 'There is no package to install here. Double-click a .faberpkg file in Files.',
  'install.retry': 'Try again',
  'install.kind.wasm': 'Code, run sandboxed (WebAssembly)',
  'install.kind.declarative': 'Settings only (declarative)',
  'install.facts.kind': 'Kind',
  'install.facts.version': 'Version',
  'install.facts.publisher': 'Publisher',
  'install.facts.publisherUnknown': 'Not given',
  'install.facts.provides': 'Provides',
  'install.replaces': 'Replaces version {version}. Parts that use it restart with the new version.',
  'install.touch.title': 'What it can touch',
  'install.touch.serial': 'A serial port you pick, at {baud} baud',
  'install.touch.bridge': 'A FaberLink bridge you pick',
  'install.touch.nothingElse': 'Nothing else: no files, no network, no other hardware.',
  'install.notChecked.title': 'Not checked by the Faber registry',
  'install.notChecked.body':
    'Install packages only from people you trust. Faber runs the connector in its sandbox, but nobody has reviewed this package.',
  'install.readme': 'About this connector',
  'install.readme.empty': 'The package has no description.',
  'install.install': 'Install',
  'install.update': 'Update',
  'install.installing': 'Installing…',
  'install.cancel': 'Cancel',
  'install.done.title': '{name} is installed',
  'install.done.body': 'Add it as a part: it is in the list of connectors in Add part now.',
  'install.done.addPart': 'Add part',
  'install.done.close': 'Close',

  // Installing an app (docs/apps.md §1): what it needs, what it may drive, what it publishes
  'install.app.kind': 'app',
  'install.app.facts.runs': 'Runs',
  'install.app.runs.logic': 'In the background, {hz} times a second, while it is turned on',
  'install.app.runs.uiOnly': 'Only in its window',
  'install.app.facts.window': 'Window',
  'install.app.window.yes': 'Yes, it opens as a window',
  'install.app.window.no': 'No window',
  'install.app.replaces': 'Replaces version {version}. Its saved settings are kept.',
  'install.app.needs.title': 'What it needs',
  'install.app.needs.intro':
    '{app} works with parts by what they do, not by name. Pick which of your parts it uses; you can change them later in Apps.',
  'install.app.needs.none': 'No parts: it only shows things.',
  'install.app.nothingElse':
    'Nothing else: no files, no network, no other parts. Its window can only talk to Faber.',
  'install.app.publishes.title': 'What it publishes',
  'install.app.notChecked.body':
    'Nobody has checked this app yet: install it only if you trust where it came from. It can only use the parts you pick here.',
  'install.app.readme': 'About this app',
  'install.app.turnOn': 'Turn it on now',
  'install.app.turnOn.needsParts': 'Pick a part for each role to turn it on.',
  'install.app.done.on': 'It is on, and runs with the parts you picked.',
  'install.app.done.off': 'It is off. Turn it on in Apps when you are ready.',
  'install.app.done.needsParts': 'Pick its parts in Apps, then turn it on.',
  'install.app.done.apps': 'Open Apps',
  'install.app.done.open': 'Open its window',

  // Picking the parts for an app's roles
  'roles.title': 'Roles',
  'roles.count.one': 'one part',
  'roles.count.many': 'one or more parts',
  'roles.count.optional': 'one part, optional',
  'roles.count.any': 'any number of parts, optional',
  'roles.noPart':
    'No part provides <code>{capability}</code> yet. Add one in Parts, then pick it in Apps.',
  'roles.pickLater': 'Pick later',
  'roles.none': 'None',
  'roles.willDrive':
    '<b>{app} will be able to drive {parts}.</b> It moves it whenever the app is on.',
  'roles.willDrivePicked': '<b>{app} will be able to drive the part you pick here.</b>',

  // An app window's page asking the shell (docs/apps.md §9): the errors it gets back
  'appHost.badRequest.message': 'The app sent a message Faber can’t read.',
  'appHost.badRequest.hint': 'This is a bug in the app: tell its author.',
  'appHost.unknownRequest.message': 'The app asked for “{name}”, which Faber doesn’t know.',
  'appHost.unknownRequest.hint': 'The app may need a newer Faber. Tell its author.',
  'appHost.unsupportedVersion.message': 'The app speaks another version of Faber’s app protocol.',
  'appHost.unsupportedVersion.hint': 'Update Faber, or install the version of the app made for it.',
  'appHost.unknownRole.message': 'The app has no role called “{role}”.',
  'appHost.unknownRole.hint': 'An app can only use the roles its faber.toml asks for.',
  'appHost.unknownSignal.message': 'The app doesn’t publish a signal called “{name}”.',
  'appHost.unknownSignal.hint': 'It can read only the signals in its faber.toml’s [[publishes]].',
  'appHost.noPart.message': 'No part fills the role “{role}” right now.',
  'appHost.noPart.hint': 'Pick a part for it in Apps, and check that the part is running.',
  'appHost.notCommandable.message': 'The app may not drive the role “{role}”.',
  'appHost.notCommandable.hint': 'Only roles with command = true in its faber.toml can be driven.',
  'appHost.appOff.message': '{app} is turned off, so it can’t drive anything.',
  'appHost.appOff.hint': 'Turn it on in Apps first.',
  'appHost.notConfirmed.message': 'You didn’t let the app drive.',
  'appHost.notConfirmed.hint': 'Try again and choose Start when Faber asks.',
  'appHost.leaseRequired.message': 'The app isn’t driving “{role}”.',
  'appHost.leaseRequired.hint': 'Start driving it first.',
  'appHost.badCommand.message': 'That isn’t a command for “{role}”.',
  'appHost.badCommand.hint': 'Send a command of the role’s capability, as in docs/api.md.',
  'appHost.tooManyRequests.message': 'The app sent too many messages at once.',
  'appHost.tooManyRequests.hint': 'Send at most 200 a second; 20 commands a second is plenty.',
  'appHost.tooManySubscriptions.message': 'The app follows too many signals at once.',
  'appHost.tooManySubscriptions.hint': 'Stop one first; a window may follow 16.',
  'appHost.badKey.message': 'That state key is empty or longer than 256 bytes.',
  'appHost.badKey.hint': 'Use a short name for each value, like "speed".',
  'appHost.stateFull.message': "The app's saved state would be larger than 64 KB.",
  'appHost.stateFull.hint': 'Save less, or remove values the app no longer needs.',
  'appHost.appGone.message': 'The app isn’t installed any more, or its page changed.',
  'appHost.appGone.hint': 'Close its window, and open it again from Apps.',
  'appHost.leaseEnded.message': 'The app stopped driving “{role}”.',
  'appHost.leaseEnded.hint': 'Start driving it again when you are ready.',
  'appHost.stopped.message': 'You stopped the app driving “{role}”.',
  'appHost.stopped.hint': 'Start driving it again when you are ready.',

  // An app's window
  'appWindow.loading': 'Opening the app…',
  'appWindow.missing': 'This app isn’t installed any more.',
  'appWindow.noUi': '{app} has no window. It runs in the background while it is on.',
  'appWindow.frameTitle': '{app}',
  'appWindow.driving': '{app} is driving {parts}.',
  'appWindow.stop': 'Stop',
  'appWindow.confirm.body':
    '{app} wants to drive “{part}”. It can make it move until you press Stop or close its window. The Stop button on the taskbar stops everything.',

  // Apps: every installed app, on and off, its parts and its window
  'apps.title': 'Apps',
  'apps.loading': 'Loading apps…',
  'apps.empty': 'No apps installed yet. Double-click a .faberpkg file in Files to install one.',
  'apps.meta': 'version {version}',
  'apps.on': 'On',
  'apps.off': 'Off',
  'apps.onNamed': 'Turn {name} on',
  'apps.open': 'Open',
  'apps.openNamed': 'Open {name}',
  'apps.uninstall': 'Uninstall',
  'apps.uninstallNamed': 'Uninstall {name}',
  'apps.uninstall.whileOn': 'Turn it off to uninstall it, so nothing it drives stops by surprise.',
  'apps.uninstall.title': 'Uninstall {name}?',
  'apps.uninstall.body':
    'Its window, its saved settings and its parts in apps.toml go with it. You can install the package again later.',
  'apps.roles': 'Its parts',
  'apps.roles.named': 'Parts of {name}',
  'apps.roles.save': 'Save parts',
  'apps.roles.restarts': 'It restarts with the new parts.',
  'apps.publishes': 'What it publishes',
  'apps.publishes.named': 'What {name} publishes',
  'apps.problems.title': 'Apps that could not be loaded',

  // Derived signals (D24)
  'signals.appBadge': 'app',
  'signals.appBadge.title': 'Computed and published by the app {app}, not a part',

  // E-stop (taskbar)
  'estop.stop': 'Stop',
  'estop.stop.title': 'Stop everything that moves (Pause/Break)',
  'estop.engaged': 'Stopped',
  'estop.engaged.detail': 'The e-stop is engaged: {reason}',
  'estop.release': 'Release',
  'estop.release.title': 'Release the e-stop?',
  'estop.release.body':
    'Parts can move again once it is released. Check that nothing and nobody is in the way.',
  'estop.release.confirm': 'Release',
  'estop.release.cancel': 'Keep stopped',
  'estop.reason.taskbar': 'Stop button on the taskbar',
  'estop.reason.key': 'Pause/Break key',
  'estop.reason.drive': 'Stop button in the Drive window',

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
  'files.type.package': 'Connector package',

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

  // Arduino flasher (docs/api.md, "The Arduino flasher")
  'flasher.port.label': 'Port',
  'flasher.port.none': 'No ports found. Plug the board in with its USB cable.',
  'flasher.port.option': '{label} ({id})',
  'flasher.port.optionGuess': '{option} · {guess}',
  'flasher.port.optionBusy': '{option} · {reason}',
  'flasher.port.clone': '{chip} USB chip: pick the board',
  'flasher.board.label': 'Board',
  'flasher.board.pick': 'Pick the board',
  'flasher.board.option': '{name} ({chip})',
  'flasher.board.guessed': 'Its USB ids say it is an {board}.',
  'flasher.board.clone':
    'Clone boards use this USB chip, so Faber can’t tell which board it is. Pick the one you have.',
  'flasher.firmware.label': 'Program',
  'flasher.firmware.file': 'A .hex file from Files',
  'flasher.firmware.package': 'Firmware from a package',
  'flasher.firmware.choose': 'Choose…',
  'flasher.firmware.noFile': 'No file chosen',
  'flasher.firmware.packageLabel': 'Package',
  'flasher.firmware.noPackages': 'No installed package has firmware for the {board}.',
  'flasher.firmware.packageOption': '{name} {version}',
  'flasher.firmware.packageFile': '{package}: firmware/{board}.hex',
  'flasher.firmware.help':
    'In the Arduino IDE: Sketch → Export Compiled Binary, then copy the .hex without “with_bootloader” in its name into Files.',
  'flasher.warning.title': 'This replaces the program on the board',
  'flasher.warning.body':
    'Whatever the board runs now is gone afterwards. Faber checks that the bootloader answers and that the chip is the board you picked before it writes anything.',
  'flasher.flash': 'Flash',
  'flasher.confirm.title': 'Replace the program on the board?',
  'flasher.confirm.body':
    'Faber puts <code>{firmware}</code> on the {board} on <code>{port}</code>. The program on it now is replaced.',
  'flasher.confirm.cancel': 'Cancel',
  'flasher.confirm.flash': 'Replace and flash',
  'flasher.stage.connecting': 'Waiting for the bootloader…',
  'flasher.stage.writing': 'Writing… {percent} %',
  'flasher.stage.verifying': 'Checking… {percent} %',
  'flasher.stage.done': 'Done',
  'flasher.progress.label': 'Flashing progress',
  'flasher.progress.keepPlugged': 'Keep the board plugged in until it is done.',
  'flasher.done.title': 'The {board} runs the new program.',
  'flasher.done.body': '{size} written and checked.',
  'flasher.done.restarted': '“{part}” starts again by itself, on the new firmware.',
  'flasher.done.check': 'Check for a FaberLink board',
  'flasher.done.checking': 'Checking…',
  'flasher.done.again': 'Flash again',
  'flasher.bridge.stop': 'Stop the board and flash',

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
  'settings.nav.connectors': 'Connectors',

  // Settings → Connectors (Advanced): installed packages and connectors that couldn't be loaded
  'settings.connectors.title': 'Connectors',
  'settings.connectors.intro':
    'Connector packages installed on this machine. Install one by double-clicking a .faberpkg file in Files. The built-in connectors are always there.',
  'settings.connectors.empty': 'No packages installed.',
  'settings.connectors.loading': 'Loading packages…',
  'settings.connectors.meta': '{kind} · version {version} · installed from <code>{source}</code>',
  'settings.connectors.publisher': 'by {publisher}',
  'settings.connectors.notChecked': 'Not checked by the Faber registry',
  'settings.connectors.usedBy': {
    one: 'Used by {count} part, {parts}. Remove it in Parts to uninstall.',
    other: 'Used by {count} parts, {parts}. Remove them in Parts to uninstall.',
  },
  'settings.connectors.uninstall': 'Uninstall',
  'settings.connectors.uninstall.named': 'Uninstall {name}',
  'settings.connectors.uninstall.title': 'Uninstall {name}?',
  'settings.connectors.uninstall.body':
    'Its connector disappears from Add part. You can install the package again later.',
  'settings.connectors.uninstall.confirm': 'Uninstall',
  'settings.connectors.uninstall.cancel': 'Cancel',
  'settings.connectors.problems.title': 'Connectors that could not be loaded',
  'settings.connectors.problems.intro':
    'Faber runs without these. Each says what is wrong in <code>connectors/</code> in the data folder.',
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
    'Saves parts, settings, installed connectors, Desktop and Documents as one .zip in Backups. Download it to keep a copy off this machine.',
  'settings.storage.backup.recordings': 'Include recordings',
  'settings.storage.backup.recordingsSize': 'Include recordings ({size})',
  'settings.storage.backup.now': 'Back up now',
  'settings.storage.backup.busy': 'Backing up…',
  'settings.storage.backup.saved': 'Saved {name} ({size}).',
  'settings.storage.setup.name': "Share this machine's setup",
  'settings.storage.setup.desc':
    "Saves parts, settings, installed connectors and apps as one .zip in Backups, without your files, this machine's name or its developer settings. Restore it on another machine to set it up the same way.",
  'settings.storage.setup.now': 'Share setup…',
  'settings.storage.setup.busy': 'Saving…',
  'settings.storage.setup.saved':
    'Saved {name} ({size}). Download it, put it in the other machine’s Backups folder and restore it there.',
  'settings.storage.setup.restoreTitle': 'Set up this machine from this setup?',
  'settings.storage.setup.restoreBody':
    'Parts, apps, installed connectors and settings become the ones in the setup. This machine keeps its name, its files and its developer settings.',
  'settings.storage.setup.restored': 'Set up from {name}. This machine kept its name and files.',
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
