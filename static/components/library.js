import { escapeHtml, getStatusTransition, normalizeFileStatus, normalizeFileType } from '../utils.js';

export function createLibraryComponent(context, actions) {
    const { state, elements } = context;

    function drawerMatches(file) {
        if (state.currentDrawerFilter === 'all-published') return true;
        if (state.currentDrawerFilter === 'unclassified') return !file.drawer_id;
        return Number(file.drawer_id) === Number(state.currentDrawerFilter);
    }

    function getDrawer(id) {
        return state.drawers.find(drawer => Number(drawer.id) === Number(id));
    }

    function renderLibraryGrid() {
        elements.drawerBar.classList.toggle('hidden', state.currentFilter !== 'published');
        elements.libraryGrid.innerHTML = '';
        const filteredFiles = state.allFiles.filter(file => {
            const status = normalizeFileStatus(file.status);
            let matchesFilter = true;
            if (state.currentFilter === 'published') {
                matchesFilter = status === 'published' && drawerMatches(file);
            } else if (state.currentFilter === 'draft' || state.currentFilter === 'archived') {
                matchesFilter = status === state.currentFilter;
            }
            const searchText = [
                file.original_name || '',
                ...(file.tags || []).map(tag => tag.label || '')
            ].join(' ').toLowerCase();
            const matchesSearch = searchText.includes(state.currentSearch.toLowerCase());
            return matchesFilter && matchesSearch;
        });
        elements.libraryStats.textContent = `총 ${filteredFiles.length}개의 지식이 저장되어 있습니다.`;

        filteredFiles.forEach(file => {
            const card = document.createElement('div');
            card.className = `library-card glass ${state.currentFilename === file.filename ? 'active' : ''} ${state.selectedFiles.has(file.filename) ? 'selected' : ''}`;
            const uploadedAt = new Date(file.uploaded_at);
            const date = relativeTime(uploadedAt);
            const fullDate = Number.isNaN(uploadedAt.getTime()) ? '' : uploadedAt.toLocaleString();

            const header = document.createElement('div');
            header.className = 'card-header';
            const titleGroup = document.createElement('div');
            titleGroup.className = 'card-title-group';
            const checkbox = document.createElement('input');
            checkbox.type = 'checkbox';
            checkbox.className = 'file-checkbox';
            checkbox.checked = state.selectedFiles.has(file.filename);
            checkbox.dataset.filename = file.filename || '';
            titleGroup.appendChild(checkbox);

            const title = document.createElement('div');
            title.className = 'card-title';
            title.textContent = file.original_name || '';
            title.title = file.original_name || '';
            titleGroup.appendChild(title);

            const moreActions = document.createElement('span');
            moreActions.className = 'more-actions-btn';
            moreActions.dataset.filename = file.filename || '';
            moreActions.textContent = '⋮';

            header.appendChild(titleGroup);
            header.appendChild(moreActions);

            const meta = document.createElement('div');
            meta.className = 'card-meta';
            const typeBadge = document.createElement('span');
            const fileType = normalizeFileType(file.file_type);
            typeBadge.className = `type-badge type-${fileType}`;
            typeBadge.textContent = fileType === 'md' ? 'MD' : 'HTML';
            meta.appendChild(typeBadge);

            const statusGroup = document.createElement('div');
            statusGroup.style.display = 'flex';
            statusGroup.style.gap = '10px';
            statusGroup.style.alignItems = 'center';
            statusGroup.style.flexWrap = 'wrap';
            statusGroup.style.justifyContent = 'flex-end';

            const statusBadge = document.createElement('span');
            const normalizedStatus = normalizeFileStatus(file.status);
            const transition = getStatusTransition(normalizedStatus);
            statusBadge.className = `status-badge status-${normalizedStatus}`;
            statusBadge.dataset.filename = file.filename || '';
            statusBadge.dataset.status = normalizedStatus;
            statusBadge.title = transition.title;
            statusBadge.setAttribute('aria-label', transition.ariaLabel);
            if (transition.nextStatus) {
                statusBadge.setAttribute('role', 'button');
                statusBadge.tabIndex = 0;
                statusBadge.addEventListener('keydown', (e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        e.stopPropagation();
                        actions.toggleStatus(statusBadge);
                    }
                });
            } else {
                statusBadge.setAttribute('role', 'status');
                statusBadge.classList.add('status-static');
            }
            statusBadge.textContent = file.status || '';
            statusGroup.appendChild(statusBadge);

            const drawer = getDrawer(file.drawer_id);
            if (normalizeFileStatus(file.status) === 'published' && drawer) {
                const drawerBadge = document.createElement('span');
                drawerBadge.className = 'drawer-badge';
                drawerBadge.textContent = drawer.name;
                drawerBadge.title = drawer.name;
                statusGroup.appendChild(drawerBadge);
            }

            const fileDate = document.createElement('span');
            fileDate.className = 'file-date';
            fileDate.textContent = date;
            fileDate.title = fullDate;
            statusGroup.appendChild(fileDate);

            meta.appendChild(statusGroup);
            card.appendChild(header);
            card.appendChild(meta);

            const chips = renderTagChips(file.tags || []);
            if (chips) card.appendChild(chips);

            if (fileType === 'md' && file.content_preview) {
                const preview = document.createElement('p');
                preview.className = 'card-preview';
                preview.textContent = file.content_preview;
                card.appendChild(preview);
            }

            checkbox.addEventListener('click', (e) => {
                e.stopPropagation();
                setFileSelection(file.filename, e.target.checked, card, checkbox);
            });

            card.addEventListener('click', (e) => {
                if (e.target.classList.contains('status-badge') && e.target.getAttribute('role') === 'button') {
                    e.stopPropagation();
                    actions.toggleStatus(e.target);
                    return;
                }
                if (e.target.classList.contains('more-actions-btn')) {
                    e.stopPropagation();
                    state.mobileActionTarget = file;
                    document.getElementById('action-sheet-title').textContent = file.original_name;
                    actions.configureActionSheet(file);
                    elements.mobileActionSheet.classList.remove('hidden');
                    setTimeout(() => elements.mobileActionSheet.classList.add('show'), 10);
                    return;
                }
                if (state.selectedFiles.size > 0) {
                    e.preventDefault();
                    setFileSelection(file.filename, !state.selectedFiles.has(file.filename), card, checkbox);
                    return;
                }
                actions.loadContent(file.filename);
            });
            elements.libraryGrid.appendChild(card);
        });
    }

    function setFileSelection(filename, selected, card, checkbox) {
        if (!filename) return;
        if (selected) state.selectedFiles.add(filename);
        else state.selectedFiles.delete(filename);
        checkbox.checked = state.selectedFiles.has(filename);
        card.classList.toggle('selected', checkbox.checked);
        if (state.selectedFiles.size > 0 && actions.collapseSelectionBar) {
            actions.collapseSelectionBar();
        } else {
            actions.updateSelectionUI();
        }
    }

    function renderTagChips(tags) {
        if (!tags.length) return null;
        const wrap = document.createElement('div');
        wrap.className = 'tag-chip-row';
        tags.slice(0, 6).forEach(tag => {
            const chip = document.createElement('span');
            chip.className = `tag-chip tag-${tag.kind === 'user' ? 'user' : 'system'}`;
            chip.textContent = tag.label || '';
            chip.title = tag.source ? `${tag.label} · ${tag.source}` : tag.label || '';
            wrap.appendChild(chip);
        });
        if (tags.length > 6) {
            const extra = document.createElement('span');
            extra.className = 'tag-chip tag-extra';
            extra.textContent = `+${tags.length - 6}`;
            wrap.appendChild(extra);
        }
        return wrap;
    }

    function relativeTime(date) {
        const timestamp = date.getTime();
        if (Number.isNaN(timestamp)) return '';

        const diffSeconds = Math.round((timestamp - Date.now()) / 1000);
        const absSeconds = Math.abs(diffSeconds);
        const units = [
            { limit: 60, value: 1, suffix: '초' },
            { limit: 3600, value: 60, suffix: '분' },
            { limit: 86400, value: 3600, suffix: '시간' },
            { limit: 2592000, value: 86400, suffix: '일' },
            { limit: 31536000, value: 2592000, suffix: '개월' },
            { limit: Infinity, value: 31536000, suffix: '년' }
        ];
        const unit = units.find(unit => absSeconds < unit.limit);
        const amount = Math.max(1, Math.round(absSeconds / unit.value));
        return diffSeconds > 0 ? `${amount}${unit.suffix} 후` : `${amount}${unit.suffix} 전`;
    }

    function renderDrawerList() {
        elements.drawerList.innerHTML = '';
        const entries = [
            { id: 'all-published', name: 'All published', count: state.allFiles.filter(f => normalizeFileStatus(f.status) === 'published').length },
            { id: 'unclassified', name: 'Unclassified', count: state.allFiles.filter(f => normalizeFileStatus(f.status) === 'published' && !f.drawer_id).length }
        ].concat(state.drawers.map(drawer => ({ ...drawer, id: String(drawer.id), count: drawer.file_count })));

        entries.forEach(entry => {
            const item = document.createElement('div');
            item.className = `drawer-chip ${String(state.currentDrawerFilter) === String(entry.id) ? 'active' : ''}`;
            item.setAttribute('role', 'button');
            item.tabIndex = 0;
            item.dataset.drawerFilter = entry.id;
            item.innerHTML = `<span>${escapeHtml(entry.name)}</span><strong>${entry.count || 0}</strong>`;
            let longPressTimer = null;
            let longPressHandled = false;
            const selectDrawer = () => {
                if (longPressHandled) {
                    longPressHandled = false;
                    return;
                }
                state.currentDrawerFilter = entry.id;
                if (state.currentFilter !== 'published') {
                    state.currentFilter = 'published';
                    elements.filterButtons.forEach(b => b.classList.toggle('active', b.dataset.filter === 'published'));
                }
                renderDrawerList();
                renderLibraryGrid();
            };
            item.addEventListener('click', selectDrawer);
            item.addEventListener('keydown', (e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    selectDrawer();
                }
            });

            if (!['all-published', 'unclassified'].includes(entry.id)) {
                const startLongPress = () => {
                    longPressHandled = false;
                    clearTimeout(longPressTimer);
                    longPressTimer = setTimeout(async () => {
                        longPressHandled = true;
                        await actions.renameDrawer(Number(entry.id));
                    }, 650);
                };
                const cancelLongPress = () => clearTimeout(longPressTimer);
                item.addEventListener('pointerdown', startLongPress);
                item.addEventListener('pointerup', cancelLongPress);
                item.addEventListener('pointerleave', cancelLongPress);
                item.addEventListener('pointercancel', cancelLongPress);
                item.addEventListener('contextmenu', async (e) => {
                    e.preventDefault();
                    await actions.renameDrawer(Number(entry.id));
                });
            }
            elements.drawerList.appendChild(item);

            if (!['all-published', 'unclassified'].includes(entry.id)) {
                const actionRow = document.createElement('div');
                actionRow.className = 'drawer-chip-actions';
                const remove = document.createElement('button');
                remove.textContent = '×';
                remove.title = 'Delete drawer';
                remove.setAttribute('aria-label', `Delete drawer ${entry.name}`);
                remove.addEventListener('pointerdown', (e) => e.stopPropagation());
                remove.addEventListener('contextmenu', (e) => e.stopPropagation());
                remove.addEventListener('click', (e) => { e.stopPropagation(); actions.deleteDrawer(Number(entry.id)); });
                actionRow.appendChild(remove);
                item.appendChild(actionRow);
            }
        });
    }

    function renderActionDrawerSelect() {
        const options = state.drawers.map(drawer => `<option value="${drawer.id}">${escapeHtml(drawer.name)}</option>`).join('');
        elements.actionDrawerSelect.innerHTML = options;
        elements.selectionDrawerSelect.innerHTML = options;
    }

    return { renderLibraryGrid, renderDrawerList, renderActionDrawerSelect, getDrawer };
}

export function renderLibraryGrid(context, actions) {
    return createLibraryComponent(context, actions).renderLibraryGrid();
}

export function renderDrawerList(context, actions) {
    return createLibraryComponent(context, actions).renderDrawerList();
}
