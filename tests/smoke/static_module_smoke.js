const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..', '..');

const moduleExpectations = [
  {
    file: 'static/app.js',
    imports: ['./controllers/notifications.js'],
    exports: [],
  },
  {
    file: 'static/state.js',
    imports: [],
    exports: ['state', 'elements', 'initElements', 'createContext'],
    behaviorTokens: ['selectionChip', 'selectionBarCollapsed', 'scrapModeSelect'],
  },
  {
    file: 'static/components/library.js',
    imports: ['../utils.js'],
    exports: ['createLibraryComponent', 'renderLibraryGrid', 'renderDrawerList'],
    behaviorTokens: ['document.createElement', 'elements.libraryGrid', 'elements.drawerList', 'setFileSelection(file.filename', 'state.selectedFiles.size > 0', "card.classList.toggle('selected'", 'actions.collapseSelectionBar'],
  },
  {
    file: 'static/components/viewer.js',
    imports: ['../utils.js', './modals.js'],
    exports: ['renderViewerContent', 'scrollViewerTo', 'showResearchRequest', 'showRelationshipGraph', 'copyResearchRequestTitle', 'copyViewerContent'],
    behaviorTokens: ['document.createElement', 'document.createElementNS', 'api.getRawFileContent', 'api.getResearchRequest', 'api.getRelationshipGraph', 'normalizeRelationshipGraph', 'graphLinksForDirection', 'iframe.onload', 'source_documents', 'relationships.derivatives', 'research-request-relationship-row', 'relationship-graph-edge-label', 'relationTypeLabel', 'encodeURIComponent(relation.document.filename)', 'encodeURIComponent(filename)', 'actions.loadContent(source.filename)', 'actions.loadContent(relation.document.filename)'],
  },
  {
    file: 'static/components/modals.js',
    imports: [],
    exports: ['openModal', 'closeModal', 'bindBackdropClose'],
    behaviorTokens: ['classList.remove', 'classList.add', 'addEventListener'],
  },
  {
    file: 'static/controllers/presets.js',
    imports: ['../utils.js'],
    exports: ['initPresetsController'],
    behaviorTokens: ['api.listEnginePresets', 'api.createEnginePreset', 'elements.presetDetailList'],
  },
  {
    file: 'static/controllers/drawers.js',
    imports: ['../utils.js'],
    exports: ['initDrawersController'],
    behaviorTokens: ['api.createDrawer', 'api.updateFileDrawer', 'configureActionSheet'],
  },
  {
    file: 'static/controllers/tasks.js',
    imports: [],
    exports: ['initTasksController'],
    behaviorTokens: ['new EventSource', 'api.listTasks', 'renderTaskList'],
  },
  {
    file: 'static/controllers/research.js',
    imports: [],
    exports: ['initResearchController'],
    behaviorTokens: ['api.startTopicResearch', 'api.startFileResearch', 'api.translateFile'],
  },
  {
    file: 'static/controllers/scraping.js',
    imports: [],
    exports: ['initScrapingController'],
    behaviorTokens: ['api.scrapUrl', 'api.uploadFile', 'FormData', "elements.scrapModeSelect.value = 'general'", 'mode = elements.scrapModeSelect.value'],
  },
  {
    file: 'static/controllers/notifications.js',
    imports: [
      '../utils.js',
      '../api.js',
      '../state.js',
      '../components/modals.js',
      '../components/library.js',
      '../components/viewer.js',
      './drawers.js',
      './presets.js',
      './research.js',
      './scraping.js',
      './tasks.js',
    ],
    exports: ['initApp'],
    behaviorTokens: ['decodeURIComponent', 'encodeURIComponent(filename)', 'showRelationshipGraph(context, actions)', "elements.libraryView.classList.toggle('selection-active'", 'collapseSelectionBar', 'expandSelectionBar', "elements.selectionCount.classList.toggle('is-collapsed'"],
  },
];

const compositionExpectations = [
  ['static/controllers/notifications.js', 'createContext(api)'],
  ['static/controllers/notifications.js', 'createLibraryComponent(context, actions)'],
  ['static/controllers/notifications.js', 'initTasksController(context, actions)'],
  ['static/controllers/notifications.js', 'initDrawersController(context, actions)'],
  ['static/controllers/notifications.js', 'initPresetsController(context, actions)'],
  ['static/controllers/notifications.js', 'initResearchController(context, actions)'],
  ['static/controllers/notifications.js', 'initScrapingController(context, actions)'],
  ['static/controllers/notifications.js', 'renderViewerContent(context, filename, actions, push)'],
  ['static/controllers/notifications.js', 'openModal(elements.taskModal)'],
];

const movedOutOfNotifications = [
  'function setupTaskSSE',
  'function renderTaskList',
  'function renderLibraryGrid',
  'function renderDrawerList',
  'function openScrapModal',
  'function scrapUrl',
  'function openResearchModal',
  'function startResearch',
  'function fetchEnginePresets',
  'function openPresetModal',
  'function createDrawer',
  'function configureActionSheet',
  'function scrollViewerTo',
  'function copyViewerContent',
];

const forbiddenNotificationAudioPatterns = [
  'assets.mixkit.co',
  "new Audio('https://",
  'new Audio("https://',
];

const taskControllerSafetyTokens = [
  'function isResearchEventObject',
  '.filter(isResearchEventObject)',
  'function normalizeResearchEvent',
  "typeof event.detail === 'string'",
  'function normalizeSourceQueries',
  '.filter(isSourceQueryObject)',
  'function normalizeSourceQuery',
  "typeof query.error === 'string'",
];

function readProjectFile(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), 'utf8');
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function hasImport(source, imported) {
  return source.includes(`from '${imported}'`) || source.includes(`from "${imported}"`);
}

function hasExport(source, exported) {
  const exportPattern = new RegExp(
    `export\\s+(async\\s+)?(const|let|function|class)\\s+${exported}\\b|export\\s*\\{[^}]*\\b${exported}\\b`,
  );
  return exportPattern.test(source);
}

function assertNoPlaceholderModule(file, source) {
  const withoutComments = source
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/\/\/.*$/gm, '');
  assert(
    !/export\s+function\s+\w+\s*\([^)]*\)\s*\{\s*return\s+null\s*;\s*\}/.test(withoutComments),
    `${file} must not contain placeholder exports`,
  );
}

for (const expectation of moduleExpectations) {
  const source = readProjectFile(expectation.file);
  for (const imported of expectation.imports) {
    assert(hasImport(source, imported), `${expectation.file} must import ${imported}`);
  }
  for (const exported of expectation.exports) {
    assert(hasExport(source, exported), `${expectation.file} must export ${exported}`);
  }
  if (expectation.behaviorTokens) {
    assertNoPlaceholderModule(expectation.file, source);
    for (const token of expectation.behaviorTokens) {
      assert(source.includes(token), `${expectation.file} must contain behavior token ${token}`);
    }
  }
}

for (const [file, token] of compositionExpectations) {
  assert(readProjectFile(file).includes(token), `${file} must compose ${token}`);
}

const notificationsSource = readProjectFile('static/controllers/notifications.js');
for (const forbidden of movedOutOfNotifications) {
  assert(!notificationsSource.includes(forbidden), `notifications.js must not retain domain logic ${forbidden}`);
}
for (const forbidden of forbiddenNotificationAudioPatterns) {
  assert(!notificationsSource.includes(forbidden), `notifications.js must not reference external notification audio ${forbidden}`);
}
assert(
  notificationsSource.includes("new Audio('/assets/notification.wav')"),
  'notifications.js must use repository-owned notification audio',
);
assert(
  fs.existsSync(path.join(root, 'static/assets/notification.wav')),
  'repository-owned notification audio asset must exist',
);

const tasksSource = readProjectFile('static/controllers/tasks.js');
for (const token of taskControllerSafetyTokens) {
  assert(tasksSource.includes(token), `tasks.js must preserve malformed research artifact guard: ${token}`);
}

const filesRustSource = readProjectFile('src/files.rs');
assert(
  filesRustSource.includes('"[AI-Research]" => vec!["Research"]'),
  'src/files.rs must normalize [AI-Research] to the Research system tag',
);
assert(
  !filesRustSource.includes('"[AI-Research]" => vec!["AI Research"]'),
  'src/files.rs must not create the old AI Research system tag for [AI-Research]',
);

const styleSource = readProjectFile('static/style.css');
assert(
  styleSource.includes('.library-view.selection-active'),
  'static/style.css must reserve bottom space while library selection is active',
);
assert(
  styleSource.includes('#selection-count {') && styleSource.includes('flex-direction: column;'),
  'static/style.css mobile selection bar rules must target #selection-count',
);
assert(
  !styleSource.includes('.selection-count { flex-direction: column'),
  'static/style.css must not use the old mobile .selection-count selector',
);
assert(
  styleSource.includes('.selection-floating-bar.is-collapsed'),
  'static/style.css must style the collapsed selection chip state',
);
assert(
  styleSource.includes('.selection-floating-bar.is-collapsed .selection-actions'),
  'static/style.css must hide selection actions while collapsed',
);

const indexSource = readProjectFile('static/index.html');
assert(
  indexSource.includes('id="selection-chip"'),
  'static/index.html must expose the selection chip expand affordance',
);
assert(
  indexSource.includes('id="scrap-mode-select"') && indexSource.includes('value="geeknews"'),
  'static/index.html must expose General and GeekNews scrape modes',
);

console.log('static module smoke passed');
