import { normalizeFileStatus } from '../utils.js';

export function initDrawersController(context, actions) {
    const { state, elements, api } = context;

    function getDrawer(id) {
        return actions.getDrawer(id);
    }

    function configureActionSheet(file) {
        const status = normalizeFileStatus(file.status);
        const isPublished = status === 'published';
        elements.drawerActionPanel.classList.toggle('hidden', !isPublished);
        elements.mobileActionAssignDrawer.disabled = !isPublished || state.drawers.length === 0;
        elements.mobileActionClearDrawer.disabled = !isPublished || !file.drawer_id;
        actions.renderActionDrawerSelect();
        if (isPublished && state.drawers.length > 0) {
            elements.actionDrawerSelect.value = file.drawer_id || state.drawers[0].id;
        }
    }

    async function createDrawer() {
        const name = elements.drawerNameInput.value.trim();
        if (!name) return;
        elements.createDrawerBtn.disabled = true;
        try {
            const response = await api.createDrawer({ name, description: null });
            if (response.ok) {
                elements.drawerNameInput.value = '';
                await actions.fetchDrawers();
                actions.showToast('Drawer created', '▤');
            } else if (response.status === 409) {
                actions.showToast('Drawer name already exists', '❌');
            }
        } finally {
            elements.createDrawerBtn.disabled = false;
        }
    }

    async function renameDrawer(id) {
        const drawer = getDrawer(id);
        if (!drawer) return;
        const name = prompt('Drawer name', drawer.name);
        if (!name || !name.trim()) return;
        const response = await api.updateDrawer(id, { name: name.trim(), description: drawer.description || null });
        if (response.ok) {
            await actions.refreshLibraryData();
            actions.showToast('Drawer renamed', '▤');
        } else if (response.status === 409) {
            actions.showToast('Drawer name already exists', '❌');
        }
    }

    async function deleteDrawer(id) {
        const drawer = getDrawer(id);
        if (!drawer) return;
        if (!confirm(`Delete drawer '${drawer.name}'? Files will move to Unclassified.`)) return;
        const response = await api.deleteDrawer(id);
        if (response.ok) {
            if (String(state.currentDrawerFilter) === String(id)) state.currentDrawerFilter = 'unclassified';
            await actions.refreshLibraryData();
            actions.showToast('Drawer deleted', '▤');
        }
    }

    async function mobileActionAssignDrawer() {
        if (!state.mobileActionTarget || !elements.actionDrawerSelect.value) return;
        const drawerId = Number(elements.actionDrawerSelect.value);
        const response = await api.updateFileDrawer(state.mobileActionTarget.filename, drawerId);
        if (response.ok) {
            actions.closeActionSheet();
            await actions.refreshLibraryData();
            actions.showToast('Drawer assigned', '▤');
        }
    }

    async function mobileActionClearDrawer() {
        if (!state.mobileActionTarget) return;
        const response = await api.updateFileDrawer(state.mobileActionTarget.filename, null);
        if (response.ok) {
            actions.closeActionSheet();
            await actions.refreshLibraryData();
            actions.showToast('Drawer removed', '▤');
        }
    }

    return { configureActionSheet, createDrawer, renameDrawer, deleteDrawer, mobileActionAssignDrawer, mobileActionClearDrawer };
}
