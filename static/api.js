const jsonHeaders = { 'Content-Type': 'application/json' };

async function getJson(url) {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Request failed: ${response.status}`);
    return response.json();
}

function postJson(url, body) {
    return fetch(url, {
        method: 'POST',
        headers: jsonHeaders,
        body: JSON.stringify(body)
    });
}

function putJson(url, body) {
    return fetch(url, {
        method: 'PUT',
        headers: jsonHeaders,
        body: JSON.stringify(body)
    });
}

export const api = {
    listTasks: () => getJson('/api/tasks'),
    getConfig: () => getJson('/api/config'),
    listFiles: () => getJson('/api/files'),
    listDrawers: () => getJson('/api/drawers'),
    listEnginePresets: () => getJson('/api/engine-presets'),
    listTags: () => getJson('/api/tags'),
    searchFiles: (query) => getJson(`/api/search?q=${encodeURIComponent(query)}`),
    listModels: () => getJson('/api/models'),
    getRawFileContent: (filename) => fetch(`/api/files/${encodeURIComponent(filename)}/raw?t=${Date.now()}`),
    getResearchRequest: (filename) => getJson(`/api/files/${encodeURIComponent(filename)}/research-request`),
    getRelationships: (filename) => getJson(`/api/files/${encodeURIComponent(filename)}/relationships`),
    getRelationshipGraph: (filename, { depth = 1, direction = 'both' } = {}) => getJson(`/api/files/${encodeURIComponent(filename)}/relationship-graph?depth=${encodeURIComponent(depth)}&direction=${encodeURIComponent(direction)}`),
    scrapUrl: (payload) => postJson('/api/scrap', payload),
    startTopicResearch: (payload) => postJson('/api/research/topic', payload),
    startFileResearch: (payload) => postJson('/api/files/research', payload),
    translateFile: (filename, payload) => postJson(`/api/files/${encodeURIComponent(filename)}/translate`, payload),
    uploadFile: (formData) => fetch('/api/upload', { method: 'POST', body: formData }),
    createDrawer: (payload) => postJson('/api/drawers', payload),
    createEnginePreset: (payload) => postJson('/api/engine-presets', payload),
    updateEnginePreset: (id, payload) => putJson(`/api/engine-presets/${encodeURIComponent(id)}`, payload),
    deleteEnginePreset: (id) => fetch(`/api/engine-presets/${encodeURIComponent(id)}`, { method: 'DELETE' }),
    testEnginePreset: (id) => postJson(`/api/engine-presets/${encodeURIComponent(id)}/test`, {}),
    updateDrawer: (id, payload) => putJson(`/api/drawers/${encodeURIComponent(id)}`, payload),
    deleteDrawer: (id) => fetch(`/api/drawers/${encodeURIComponent(id)}`, { method: 'DELETE' }),
    deleteFile: (filename) => fetch(`/api/files/${encodeURIComponent(filename)}`, { method: 'DELETE' }),
    updateFileStatus: (filename, status) => putJson(`/api/files/${encodeURIComponent(filename)}/status`, { status }),
    updateFileDrawer: (filename, drawerId) => putJson(`/api/files/${encodeURIComponent(filename)}/drawer`, { drawer_id: drawerId }),
    updateFileTitle: (filename, title) => putJson(`/api/files/${encodeURIComponent(filename)}/title`, { title }),
    updateFileMetadata: (filename, title, tags) => putJson(`/api/files/${encodeURIComponent(filename)}/metadata`, { title, tags }),
    updateFileTags: (filename, tags) => putJson(`/api/files/${encodeURIComponent(filename)}/tags`, { tags }),
    deleteTask: (id) => fetch(`/api/tasks/${encodeURIComponent(id)}`, { method: 'DELETE' }),
    retryTask: (id, payload = {}) => postJson(`/api/tasks/${encodeURIComponent(id)}/retry`, payload)
};
