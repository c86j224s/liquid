import { debounce, getStatusTransition, normalizeFileStatus } from '../utils.js';
import { api } from '../api.js';
import { createContext, initElements } from '../state.js';
import { bindBackdropClose, closeModal, openModal } from '../components/modals.js';
import { createLibraryComponent } from '../components/library.js';
import {
    copyResearchRequestTitle,
    copyViewerContent,
    renderViewerContent,
    scrollViewerTo,
    showRelationshipGraph,
    showResearchRequest,
    updateViewerTitle
} from '../components/viewer.js';
import { initDrawersController } from './drawers.js';
import { initPresetsController } from './presets.js';
import { initResearchController } from './research.js';
import { initScrapingController } from './scraping.js';
import { initTasksController } from './tasks.js';

export function initApp() {
    initElements();
    const context = createContext(api);
    const { state, elements } = context;
    const alertSound = new Audio('/assets/notification.wav');
    alertSound.volume = 0.2;

    const actions = {
        alertSound,
        showToast,
        sendSystemNotification,
        showErrorMessage,
        refreshLibraryData,
        fetchTags,
        fetchDrawers,
        loadContent,
        switchView,
        resetActivityTimer,
        updateSelectionUI,
        collapseSelectionBar,
        clearSelection,
        closeActionSheet,
        toggleStatus
    };
    const library = createLibraryComponent(context, actions);
    Object.assign(actions, library);
    const tasks = initTasksController(context, actions);
    Object.assign(actions, tasks);
    const drawers = initDrawersController(context, actions);
    Object.assign(actions, drawers);
    const presets = initPresetsController(context, actions);
    Object.assign(actions, presets);
    const research = initResearchController(context, actions);
    Object.assign(actions, research);
    const scraping = initScrapingController(context, actions);
    Object.assign(actions, scraping);

    init();

    function init() {
        setupEventListeners();
        tasks.setupTaskSSE();
        setupActivityTracker();
        refreshLibraryData().then(() => {
            handleInitialRoute();
        });
        presets.fetchEnginePresets();
        presets.fetchModels();
        tasks.fetchConfig();
        if (typeof Notification !== 'undefined' && Notification.permission === 'default') {
            Notification.requestPermission();
        }

        window.addEventListener('popstate', (event) => {
            if (event.state) {
                const { view, filename } = event.state;
                if (view === 'viewer' && filename) {
                    loadContent(filename, false);
                    switchView('viewer', filename, false);
                } else {
                    switchView('library', null, false);
                }
            } else {
                handleInitialRoute();
            }
        });
    }

    function handleInitialRoute() {
        const hash = window.location.hash;
        if (hash.startsWith('#/viewer/')) {
            const filename = decodeRouteFilename(hash.replace('#/viewer/', ''));
            const fileExists = state.allFiles.some(f => f.filename === filename);
            if (fileExists) {
                loadContent(filename, false);
                switchView('viewer', filename, false);
            } else {
                switchView('library', null, false);
            }
        } else {
            switchView('library', null, false);
        }
    }

    function setupEventListeners() {
        elements.appHomeLink.addEventListener('click', () => {
            state.isSearchActive = false;
            elements.toggleSearchBtn.classList.remove('active');
            elements.appContainer.classList.remove('search-active');
            state.currentSearch = '';
            elements.searchInput.value = '';
            switchView('library');
            refreshLibraryData();
        });
        elements.toggleSearchBtn.addEventListener('click', toggleSearch);
        elements.toggleTasksBtn.addEventListener('click', () => {
            tasks.fetchTasks();
            openModal(elements.taskModal);
        });
        elements.closeTaskModalBtn.addEventListener('click', () => closeModal(elements.taskModal));
        bindBackdropClose(elements.taskModal, () => closeModal(elements.taskModal));
        elements.libraryToggle.addEventListener('click', () => {
            const nextView = state.currentView === 'viewer' ? 'library' : 'viewer';
            switchView(nextView, state.currentFilename);
        });
        elements.headerScrapBtn.addEventListener('click', scraping.openScrapModal);
        elements.cancelScrapBtn.addEventListener('click', () => closeModal(elements.scrapModal));
        elements.cancelTranslateBtn.addEventListener('click', () => closeModal(elements.translateModal));
        bindBackdropClose(elements.retryModal, tasks.closeRetryModal);
        elements.closeActionSheetBtn.addEventListener('click', closeActionSheet);
        elements.mobileActionResearch.addEventListener('click', mobileActionResearch);
        elements.mobileActionFollowUp.addEventListener('click', mobileActionFollowUp);
        elements.mobileActionTranslate.addEventListener('click', mobileActionTranslate);
        elements.mobileActionEdit.addEventListener('click', mobileActionEdit);
        elements.mobileActionAssignDrawer.addEventListener('click', drawers.mobileActionAssignDrawer);
        elements.mobileActionClearDrawer.addEventListener('click', drawers.mobileActionClearDrawer);
        bindBackdropClose(elements.mobileActionSheet, closeActionSheet);
        elements.createDrawerBtn.addEventListener('click', drawers.createDrawer);
        elements.drawerNameInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') drawers.createDrawer();
        });
        elements.clearSelectionBtn.addEventListener('click', clearSelection);
        elements.selectionChip.addEventListener('click', expandSelectionBar);
        elements.libraryView.addEventListener('scroll', collapseSelectionBar);
        elements.multiResearchBtn.addEventListener('click', () => research.openResearchModal(null, null, 'synthesis'));
        elements.assignSelectionDrawerBtn.addEventListener('click', assignSelectedFilesToDrawer);
        elements.clearSelectionDrawerBtn.addEventListener('click', clearSelectedFilesDrawer);
        elements.archiveSelectionBtn.addEventListener('click', archiveSelectedFiles);
        elements.deleteSelectionBtn.addEventListener('click', deleteSelectedFiles);
        elements.newResearchBtn.addEventListener('click', () => research.openResearchModal(null, null, 'initial'));
        elements.startResearchBtn.addEventListener('click', research.startResearch);
        elements.cancelResearchBtn.addEventListener('click', () => closeModal(elements.researchModal));
        elements.startRetryBtn.addEventListener('click', tasks.startRetry);
        elements.cancelRetryBtn.addEventListener('click', tasks.closeRetryModal);
        elements.researchEnginePresetSelect.addEventListener('change', () => {
            presets.syncPresetStatusBadge();
            const preset = presets.getSelectedEnginePreset();
            if (preset && preset.default_intensity) elements.researchIntensitySelect.value = preset.default_intensity;
            research.syncQualityDefaultsFromIntensity();
        });
        elements.researchIntensitySelect.addEventListener('change', research.syncQualityDefaultsFromIntensity);
        elements.researchQualityIterationsSelect.addEventListener('change', () => {
            state.researchQualityManuallyChanged = true;
        });
        elements.researchQualityDepthSelect.addEventListener('change', () => {
            state.researchQualityManuallyChanged = true;
        });
        elements.openPresetSettingsBtn.addEventListener('click', presets.openPresetModal);
        elements.presetEditorSelect.addEventListener('change', () => {
            presets.closePresetCreatePanel();
            presets.loadPresetIntoEditor(Number(elements.presetEditorSelect.value));
        });
        elements.newPresetBtn.addEventListener('click', presets.startNewPresetFromSelected);
        elements.editPresetBtn.addEventListener('click', presets.startEditUserPreset);
        elements.deletePresetBtn.addEventListener('click', presets.deleteSelectedPreset);
        elements.cancelNewPresetBtn.addEventListener('click', presets.closePresetCreatePanel);
        elements.savePresetBtn.addEventListener('click', presets.savePreset);
        elements.testPresetBtn.addEventListener('click', presets.testSelectedPreset);
        elements.cancelPresetBtn.addEventListener('click', () => closeModal(elements.presetModal));
        elements.startTranslateBtn.addEventListener('click', research.startTranslate);
        elements.closeErrorBtn.addEventListener('click', () => closeModal(elements.errorModal));
        elements.closeResearchRequestBtn.addEventListener('click', () => closeModal(elements.researchRequestModal));
        elements.copyResearchRequestTitleBtn.addEventListener('click', () => copyResearchRequestTitle(context, actions));
        bindBackdropClose(elements.researchRequestModal, () => closeModal(elements.researchRequestModal));
        elements.relationshipGraphBtn.addEventListener('click', () => showRelationshipGraph(context, actions));
        elements.closeRelationshipGraphBtn.addEventListener('click', () => closeModal(elements.relationshipGraphModal));
        bindBackdropClose(elements.relationshipGraphModal, () => closeModal(elements.relationshipGraphModal));
        elements.viewerTitleStrip.addEventListener('click', () => showResearchRequest(context, actions));
        elements.viewerTitleStrip.addEventListener('keydown', (event) => {
            if (event.key !== 'Enter' && event.key !== ' ') return;
            event.preventDefault();
            showResearchRequest(context, actions);
        });
        elements.searchInput.addEventListener('input', debounce(handleSearchInput, 400));
        elements.saveEditBtn.addEventListener('click', saveTitleUpdate);
        elements.cancelEditBtn.addEventListener('click', () => closeModal(elements.editModal));
        elements.editTitleInput.addEventListener('input', updateCharCount);
        elements.scrapBtn.addEventListener('click', scraping.scrapUrl);
        elements.scrapTranslateBtn.addEventListener('click', scraping.scrapUrlWithTranslation);
        elements.fileUpload.addEventListener('change', scraping.handleFileUpload);
        elements.copyContentBtn.addEventListener('click', () => copyViewerContent(context, actions));
        elements.viewerScrollTopBtn.addEventListener('click', () => scrollViewerTo(context, 'top', actions));
        elements.viewerScrollBottomBtn.addEventListener('click', () => scrollViewerTo(context, 'bottom', actions));

        elements.filterButtons.forEach(btn => {
            btn.addEventListener('click', () => {
                elements.filterButtons.forEach(b => b.classList.remove('active'));
                btn.classList.add('active');
                state.currentFilter = btn.dataset.filter;
                library.renderLibraryGrid();
            });
        });
    }

    function toggleSearch() {
        state.isSearchActive = !state.isSearchActive;
        elements.toggleSearchBtn.classList.toggle('active', state.isSearchActive);
        updateSecondaryHeader();
    }

    function updateSecondaryHeader() {
        elements.searchPane.classList.toggle('hidden', !state.isSearchActive);
        const showLibActions = state.currentView === 'library' && !state.isSearchActive;
        elements.libraryActionsPane.classList.toggle('hidden', !showLibActions);
        elements.secondaryHeader.classList.toggle('library-secondary', showLibActions);
        elements.appContainer.classList.toggle('library-secondary-visible', showLibActions);

        const isVisible = state.isSearchActive || showLibActions;
        if (isVisible) {
            elements.secondaryHeader.classList.remove('hidden');
            elements.appContainer.classList.add('has-secondary');
        } else {
            elements.secondaryHeader.classList.add('hidden');
            elements.appContainer.classList.remove('has-secondary');
        }

        if (state.isSearchActive) setTimeout(() => elements.searchInput.focus(), 10);
    }

    function switchView(view, filename = state.currentFilename, push = true) {
        state.currentView = view;
        elements.contentViewer.classList.toggle('hidden', view !== 'viewer');
        elements.libraryView.classList.toggle('hidden', view !== 'library');
        elements.libraryToggle.textContent = view === 'viewer' ? '📚' : '📖';
        elements.libraryToggle.classList.toggle('active', view === 'library');
        if (view === 'library') library.renderLibraryGrid();
        updateViewerTitle(context);
        updateSecondaryHeader();
        resetActivityTimer();

        if (push) {
            const hash = view === 'viewer' && filename ? `#/viewer/${encodeURIComponent(filename)}` : '#/library';
            if (window.location.hash !== hash) {
                history.pushState({ view, filename }, '', hash);
            }
        }
    }

    function decodeRouteFilename(value) {
        try {
            return decodeURIComponent(value);
        } catch (err) {
            return value;
        }
    }

    async function fetchFiles() {
        try {
            state.allFiles = await api.listFiles();
            library.renderLibraryGrid();
            updateViewerTitle(context);
        } catch (e) { console.error('Failed to fetch files:', e); }
    }

    async function fetchDrawers() {
        try {
            state.drawers = await api.listDrawers();
            library.renderDrawerList();
            library.renderActionDrawerSelect();
        } catch (e) {
            state.drawers = [];
            console.error('Failed to fetch drawers:', e);
        }
    }

    async function refreshLibraryData() {
        state.researchRequestCache.clear();
        state.relationshipGraphCache.clear();
        await Promise.all([fetchFiles(), fetchDrawers(), fetchTags()]);
        library.renderDrawerList();
        library.renderActionDrawerSelect();
        library.renderLibraryGrid();
    }

    async function fetchTags() {
        try {
            state.availableTags = await api.listTags();
        } catch (e) {
            state.availableTags = [];
            console.error('Failed to fetch tags:', e);
        }
    }

    async function handleSearchInput(e) {
        state.currentSearch = e.target.value.trim();
        if (state.currentSearch.length > 1) {
            try {
                state.allFiles = await api.searchFiles(state.currentSearch);
                library.renderLibraryGrid();
            } catch (err) { console.error('Search failed:', err); }
        } else if (state.currentSearch.length === 0) {
            refreshLibraryData();
        } else {
            library.renderLibraryGrid();
        }
    }

    function loadContent(filename, push = true) {
        renderViewerContent(context, filename, actions, push);
    }

    function showToast(message, icon = '✅') {
        const toast = document.createElement('div');
        toast.className = 'toast';
        const iconEl = document.createElement('span');
        iconEl.className = 'toast-icon';
        iconEl.textContent = icon;
        const messageEl = document.createElement('span');
        messageEl.className = 'toast-message';
        messageEl.textContent = message;
        toast.appendChild(iconEl);
        toast.appendChild(messageEl);
        elements.toastContainer.appendChild(toast);
        setTimeout(() => {
            toast.classList.add('fade-out');
            setTimeout(() => toast.remove(), 500);
        }, 4000);
    }

    function sendSystemNotification(title, body) {
        if (typeof Notification !== 'undefined' && Notification.permission === 'granted') {
            new Notification(title, { body, icon: '/favicon.ico' });
        }
    }

    function updateSelectionUI() {
        const count = state.selectedFiles.size;
        if (count === 0) state.selectionBarCollapsed = true;
        elements.countText.textContent = `${count}개 선택됨`;
        elements.selectionCount.classList.toggle('hidden', count === 0);
        elements.selectionCount.classList.toggle('is-collapsed', count > 0 && state.selectionBarCollapsed);
        elements.selectionChip.setAttribute('aria-expanded', String(count > 0 && !state.selectionBarCollapsed));
        elements.libraryView.classList.toggle('selection-active', count > 0);
        elements.assignSelectionDrawerBtn.disabled = count === 0 || state.drawers.length === 0;
        elements.selectionDrawerSelect.disabled = count === 0 || state.drawers.length === 0;
        elements.clearSelectionDrawerBtn.disabled = count === 0;
    }

    function collapseSelectionBar() {
        if (state.selectedFiles.size === 0) return;
        state.selectionBarCollapsed = true;
        updateSelectionUI();
    }

    function expandSelectionBar() {
        if (state.selectedFiles.size === 0) return;
        state.selectionBarCollapsed = false;
        updateSelectionUI();
    }

    function clearSelection() {
        state.selectedFiles.clear();
        state.selectionBarCollapsed = true;
        updateSelectionUI();
        library.renderLibraryGrid();
    }

    function closeActionSheet() {
        elements.mobileActionSheet.classList.remove('show');
        setTimeout(() => elements.mobileActionSheet.classList.add('hidden'), 400);
    }

    function mobileActionResearch() {
        if (!state.mobileActionTarget) return;
        closeActionSheet();
        research.openResearchModal(state.mobileActionTarget.filename, state.mobileActionTarget.original_name, 'deep');
    }

    function mobileActionFollowUp() {
        if (!state.mobileActionTarget) return;
        closeActionSheet();
        research.openResearchModal(state.mobileActionTarget.filename, state.mobileActionTarget.original_name, 'follow_up');
    }

    function mobileActionTranslate() {
        if (!state.mobileActionTarget) return;
        closeActionSheet();
        state.translatingFilename = state.mobileActionTarget.filename;
        elements.translateTargetText.textContent = `'${state.mobileActionTarget.original_name}' 문서를 번역합니다.`;
        openModal(elements.translateModal);
    }

    function mobileActionEdit() {
        if (!state.mobileActionTarget) return;
        closeActionSheet();
        state.editingFilename = state.mobileActionTarget.filename;
        elements.editTitleInput.value = state.mobileActionTarget.original_name;
        elements.editTagsInput.value = userTagLabels(state.mobileActionTarget).join(', ');
        updateCharCount();
        openModal(elements.editModal);
        elements.editTitleInput.focus();
    }

    async function archiveSelectedFiles() {
        const filenames = Array.from(state.selectedFiles);
        if (filenames.length === 0) return;
        elements.archiveSelectionBtn.disabled = true;
        try {
            const results = await Promise.all(filenames.map(filename => api.updateFileStatus(filename, 'archived')));
            const failed = results.filter(response => !response.ok).length;
            await refreshLibraryData();
            clearSelection();
            showToast(failed === 0 ? `${filenames.length}개 항목 보관됨` : `${filenames.length - failed}개 보관, ${failed}개 실패`, failed === 0 ? '▣' : '⚠');
        } finally {
            elements.archiveSelectionBtn.disabled = false;
        }
    }

    async function assignSelectedFilesToDrawer() {
        const filenames = Array.from(state.selectedFiles);
        if (filenames.length === 0 || state.drawers.length === 0) return;
        const drawerId = Number(elements.selectionDrawerSelect.value);
        if (!Number.isFinite(drawerId)) return;
        elements.assignSelectionDrawerBtn.disabled = true;
        try {
            const results = await Promise.all(filenames.map(filename => api.updateFileDrawer(filename, drawerId)));
            const failed = results.filter(response => !response.ok).length;
            await refreshLibraryData();
            clearSelection();
            showToast(failed === 0 ? `${filenames.length}개 항목 서랍 할당됨` : `${filenames.length - failed}개 할당, ${failed}개 실패`, failed === 0 ? '▤' : '⚠');
        } finally {
            elements.assignSelectionDrawerBtn.disabled = false;
        }
    }

    async function clearSelectedFilesDrawer() {
        const filenames = Array.from(state.selectedFiles);
        if (filenames.length === 0) return;
        elements.clearSelectionDrawerBtn.disabled = true;
        try {
            const results = await Promise.all(filenames.map(filename => api.updateFileDrawer(filename, null)));
            const failed = results.filter(response => !response.ok).length;
            await refreshLibraryData();
            clearSelection();
            showToast(failed === 0 ? `${filenames.length}개 항목 서랍 해제됨` : `${filenames.length - failed}개 해제, ${failed}개 실패`, failed === 0 ? '▤' : '⚠');
        } finally {
            elements.clearSelectionDrawerBtn.disabled = false;
        }
    }

    async function deleteSelectedFiles() {
        const filenames = Array.from(state.selectedFiles);
        if (filenames.length === 0) return;
        if (!confirm(`선택한 ${filenames.length}개 항목을 삭제하시겠습니까?`)) return;
        elements.deleteSelectionBtn.disabled = true;
        try {
            const results = await Promise.all(filenames.map(filename => api.deleteFile(filename)));
            const failed = results.filter(response => !response.ok).length;
            await refreshLibraryData();
            clearSelection();
            showToast(failed === 0 ? `${filenames.length}개 항목 삭제됨` : `${filenames.length - failed}개 삭제, ${failed}개 실패`, failed === 0 ? '🗑' : '⚠');
        } finally {
            elements.deleteSelectionBtn.disabled = false;
        }
    }

    async function saveTitleUpdate() {
        const title = elements.editTitleInput.value.trim();
        if (title && state.editingFilename) {
            const tags = parseTagInput(elements.editTagsInput.value);
            const response = await api.updateFileMetadata(state.editingFilename, title, tags);
            if (response.ok) {
                closeModal(elements.editModal);
                await refreshLibraryData();
                showToast('문서 정보가 저장되었습니다.', '✓');
            } else {
                showToast('문서 정보 저장 실패', '⚠');
            }
        }
    }

    function userTagLabels(file) {
        return (file.tags || [])
            .filter(tag => tag.kind === 'user')
            .map(tag => tag.label)
            .filter(Boolean);
    }

    function parseTagInput(value) {
        const seen = new Set();
        return String(value || '')
            .split(',')
            .map(tag => tag.trim())
            .filter(Boolean)
            .filter(tag => {
                const key = tag.toLowerCase();
                if (seen.has(key)) return false;
                seen.add(key);
                return true;
            });
    }

    function updateCharCount() {
        elements.charCount.textContent = `${elements.editTitleInput.value.length} / 256`;
        elements.saveEditBtn.disabled = elements.editTitleInput.value.length === 0;
    }

    async function toggleStatus(badge) {
        const filename = badge.dataset.filename;
        const current = normalizeFileStatus(badge.dataset.status);
        const transition = getStatusTransition(current);
        if (!transition.nextStatus) {
            showToast('보관 항목은 더보기 메뉴에서 Published로 복귀하세요.', '▣');
            return;
        }
        const response = await api.updateFileStatus(filename, transition.nextStatus);
        if (response.ok) {
            await refreshLibraryData();
            showToast(transition.successMessage, '✅');
        }
    }

    function showErrorMessage(message) {
        elements.errorMessageText.textContent = message;
        openModal(elements.errorModal);
    }

    function setupActivityTracker() {
        const events = ['mousemove', 'mousedown', 'keydown', 'touchstart', 'scroll'];
        events.forEach(evt => {
            window.addEventListener(evt, resetActivityTimer, true);
        });
        elements.mainContent.addEventListener('scroll', resetActivityTimer);
    }

    function resetActivityTimer() {
        clearTimeout(state.hideTimer);
        elements.appContainer.classList.remove('headers-hidden');

        if (state.currentView === 'viewer' && !state.isSearchActive) {
            state.hideTimer = setTimeout(() => {
                elements.appContainer.classList.add('headers-hidden');
            }, 3000);
        }
    }
}
