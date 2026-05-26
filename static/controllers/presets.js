import { escapeHtml } from '../utils.js';

export function initPresetsController(context, actions) {
    const { state, elements, api } = context;

    async function fetchEnginePresets(selectId = null) {
        try {
            state.enginePresets = await api.listEnginePresets();
            renderEnginePresetSelect(selectId);
            renderPresetEditorSelect(selectId);
            syncPresetStatusBadge();
        } catch (e) {
            state.enginePresets = [];
            console.error('Failed to fetch engine presets:', e);
        }
    }

    async function fetchModels() {
        try {
            const models = await api.listModels();
            state.availableModels = models;
            const options = models.map(m => {
                const val = m.source === 'cli' ? `cli:${m.name}` : m.name;
                const label = m.source === 'cli' ? `[CLI] ${m.name}` : `[Ollama] ${m.name}`;
                return `<option value="${escapeHtml(val)}">${escapeHtml(label)}</option>`;
            }).join('');
            elements.translateModelSelect.innerHTML = options;
            elements.translateFileModelSelect.innerHTML = options;
            elements.researchModelSelect.innerHTML = options;
            const gemini = models.find(m => m.name === 'gemini');
            if (gemini) elements.researchModelSelect.value = 'cli:gemini';
        } catch (e) { console.error(e); }
    }

    function renderEnginePresetSelect(selectId = null) {
        const enabledPresets = state.enginePresets.filter(preset => preset.enabled !== 'false');
        elements.researchEnginePresetSelect.innerHTML = enabledPresets.map(preset => {
            const fallback = preset.fallback_execution === 'true' ? ' · fallback' : '';
            return `<option value="${preset.id}">${escapeHtml(preset.name)}${fallback}</option>`;
        }).join('');
        const selected = enabledPresets.some(preset => String(preset.id) === String(selectId))
            ? selectId
            : defaultResearchPresetId(enabledPresets);
        if (selected) elements.researchEnginePresetSelect.value = String(selected);
        const preset = getSelectedEnginePreset();
        if (preset && preset.default_intensity) elements.researchIntensitySelect.value = preset.default_intensity;
    }

    function renderPresetEditorSelect(selectId = null) {
        elements.presetEditorSelect.innerHTML = state.enginePresets.map(preset => `<option value="${preset.id}">${escapeHtml(preset.name)}</option>`).join('');
        const selected = state.enginePresets.some(preset => String(preset.id) === String(selectId))
            ? selectId
            : elements.researchEnginePresetSelect.value || firstPresetId(state.enginePresets);
        if (selected) {
            elements.presetEditorSelect.value = String(selected);
            loadPresetIntoEditor(Number(selected));
        } else {
            state.editingPresetId = null;
            elements.presetDetailList.textContent = '사용 가능한 내장 프리셋이 없습니다.';
            applyPresetStatusBadge(elements.presetTestStatus, null);
            elements.presetTestMessage.textContent = '';
        }
    }

    function getSelectedEnginePreset() {
        return state.enginePresets.find(preset => String(preset.id) === String(elements.researchEnginePresetSelect.value));
    }

    function defaultResearchPresetId(presets) {
        const defaultPreset = presets.find(preset => preset.is_default === 'true' && preset.engine_kind === 'pi_ollama');
        if (defaultPreset) return defaultPreset.id;
        return firstPresetId(presets);
    }

    function firstPresetId(presets) {
        return presets.length > 0 ? presets[0].id : null;
    }

    function presetStatusLabel(status) {
        return {
            available: '사용 가능',
            needs_setup: '연결 필요',
            failed: '실패',
            unverified: '미확인'
        }[status] || '미확인';
    }

    function applyPresetStatusBadge(element, preset) {
        const status = preset && preset.last_test_status ? preset.last_test_status : 'unverified';
        element.className = `preset-status-badge status-${status}`;
        element.textContent = presetStatusLabel(status);
        element.title = presetStatusMessage(preset);
    }

    function presetStatusMessage(preset) {
        const message = preset && preset.last_test_message ? preset.last_test_message : '';
        if (message === 'Pi runtime isolation is not configured yet.') {
            return 'Pi 격리 런타임이 아직 설정되지 않았습니다.';
        }
        const missingExecutable = message.match(/^(.+) executable was not found\.$/);
        if (missingExecutable) return `${missingExecutable[1]} 실행 파일을 찾지 못했습니다.`;
        const foundExecutable = message.match(/^(.+) executable found\.$/);
        if (foundExecutable) return `${foundExecutable[1]} 실행 파일을 찾았습니다.`;
        return message;
    }

    function syncPresetStatusBadge() {
        applyPresetStatusBadge(elements.researchPresetStatus, getSelectedEnginePreset());
    }

    function openPresetModal() {
        renderPresetEditorSelect(elements.researchEnginePresetSelect.value);
        closePresetCreatePanel();
        elements.presetModal.classList.remove('hidden');
    }

    function loadPresetIntoEditor(id) {
        const preset = state.enginePresets.find(item => Number(item.id) === Number(id));
        if (!preset) return;
        state.editingPresetId = preset.id;
        renderPresetDetails(preset);
        updatePresetActionVisibility(preset);
        applyPresetStatusBadge(elements.presetTestStatus, preset);
        elements.presetTestMessage.textContent = presetStatusMessage(preset);
    }

    function updatePresetActionVisibility(preset) {
        const isUserPreset = Boolean(preset) && Number(preset.id) > 0;
        elements.newPresetBtn.classList.toggle('hidden', isUserPreset);
        elements.editPresetBtn.classList.toggle('hidden', !isUserPreset);
        elements.deletePresetBtn.classList.toggle('hidden', !isUserPreset);
    }

    function renderPresetDetails(preset) {
        const rows = [
            ['이름', preset.name],
            ['실행 방식', preset.engine_kind],
            ['제공자', preset.provider],
            ['모델', preset.model || '-'],
            ['명령', preset.command || '-'],
            ['기본 강도', preset.default_intensity],
            ['웹검색', preset.web_search_enabled === 'true' ? '허용' : '사용 안 함'],
            ['Fallback', preset.fallback_execution === 'true' ? '사용' : '사용 안 함'],
            ['안내', preset.install_hint || '-']
        ];
        elements.presetDetailList.innerHTML = '';
        rows.forEach(([label, value]) => {
            const row = document.createElement('div');
            row.className = 'preset-detail-row';
            const labelEl = document.createElement('span');
            labelEl.className = 'preset-detail-label';
            labelEl.textContent = label;
            const valueEl = document.createElement('span');
            valueEl.className = 'preset-detail-value';
            valueEl.textContent = value;
            row.append(labelEl, valueEl);
            elements.presetDetailList.appendChild(row);
        });
    }

    function startNewPresetFromSelected() {
        const preset = state.enginePresets.find(item => Number(item.id) === Number(state.editingPresetId));
        const template = preset && Number(preset.id) < 0 ? preset : null;
        if (!template) {
            actions.showToast('내장 프리셋을 선택한 뒤 복사할 수 있습니다.', '⚙');
            return;
        }
        state.creatingPresetTemplateId = template.id;
        state.editingUserPresetId = null;
        elements.presetFormNameLabel.textContent = '새 프리셋 이름';
        elements.presetNewNameInput.value = `${template.name} 복사본`;
        elements.presetTemplateContainer.classList.remove('hidden');
        elements.presetTemplateNameInput.value = template.name;
        renderPresetModelCreateSelect(template);
        elements.presetNewModelContainer.classList.toggle('hidden', template.engine_kind === 'cli');
        elements.presetCreatePanel.classList.remove('hidden');
        showPresetFormActions('create');
        elements.presetNewNameInput.focus();
    }

    function renderPresetModelCreateSelect(template) {
        const ollamaModels = state.availableModels.filter(model => model.source === 'ollama');
        elements.presetNewModelSelect.innerHTML = '';
        if (ollamaModels.length === 0) {
            const option = document.createElement('option');
            option.value = '';
            option.textContent = template.model ? `기본값 사용 (${template.model})` : '사용 가능한 Ollama 모델 없음';
            elements.presetNewModelSelect.appendChild(option);
            elements.presetNewModelSelect.disabled = true;
            elements.presetNewModelHelp.textContent = template.model
                ? 'Ollama 모델 목록을 가져오지 못해 기준 프리셋의 기본 모델을 사용합니다.'
                : 'Ollama 모델 목록을 가져오지 못했습니다. 생성 시 서버 기본값을 사용합니다.';
            return;
        }
        ollamaModels.forEach(model => {
            const option = document.createElement('option');
            option.value = model.name;
            option.textContent = model.name;
            elements.presetNewModelSelect.appendChild(option);
        });
        if (template.model && !ollamaModels.some(model => model.name === template.model)) {
            const option = document.createElement('option');
            option.value = template.model;
            option.textContent = `현재 값 (${template.model})`;
            elements.presetNewModelSelect.appendChild(option);
        }
        elements.presetNewModelSelect.disabled = false;
        if (template.model) elements.presetNewModelSelect.value = template.model;
        elements.presetNewModelHelp.textContent = '사용 가능한 Ollama 모델 중 하나를 선택합니다. 실행 경로와 명령은 기준 프리셋을 따릅니다.';
    }

    function closePresetCreatePanel() {
        state.creatingPresetTemplateId = null;
        state.editingUserPresetId = null;
        elements.presetCreatePanel.classList.add('hidden');
        elements.savePresetBtn.textContent = '프리셋 생성';
        elements.savePresetBtn.classList.add('hidden');
        elements.cancelNewPresetBtn.classList.add('hidden');
        elements.testPresetBtn.classList.remove('hidden');
        const preset = state.enginePresets.find(item => Number(item.id) === Number(state.editingPresetId));
        updatePresetActionVisibility(preset);
    }

    function showPresetFormActions(mode) {
        elements.savePresetBtn.textContent = mode === 'edit' ? '변경 저장' : '프리셋 생성';
        elements.savePresetBtn.classList.remove('hidden');
        elements.cancelNewPresetBtn.classList.remove('hidden');
        elements.newPresetBtn.classList.add('hidden');
        elements.editPresetBtn.classList.add('hidden');
        elements.deletePresetBtn.classList.add('hidden');
        elements.testPresetBtn.classList.add('hidden');
    }

    function startEditUserPreset() {
        const preset = state.enginePresets.find(item => Number(item.id) === Number(state.editingPresetId));
        if (!preset || Number(preset.id) <= 0) return;
        state.creatingPresetTemplateId = null;
        state.editingUserPresetId = preset.id;
        elements.presetFormNameLabel.textContent = '프리셋 이름';
        elements.presetNewNameInput.value = preset.name || '';
        elements.presetTemplateContainer.classList.add('hidden');
        renderPresetModelCreateSelect(preset);
        elements.presetNewModelContainer.classList.toggle('hidden', preset.engine_kind === 'cli');
        elements.presetCreatePanel.classList.remove('hidden');
        showPresetFormActions('edit');
        elements.presetNewNameInput.focus();
    }

    async function savePreset() {
        const name = elements.presetNewNameInput.value.trim();
        if (!name) {
            actions.showToast('프리셋 이름을 입력해주세요.', '⚙');
            return;
        }
        const isEdit = Boolean(state.editingUserPresetId);
        const payload = isEdit
            ? { name, model: elements.presetNewModelSelect.disabled ? null : elements.presetNewModelSelect.value || null }
            : { name, template_id: state.creatingPresetTemplateId, model: elements.presetNewModelSelect.disabled ? null : elements.presetNewModelSelect.value || null };
        if (!isEdit && !state.creatingPresetTemplateId) {
            actions.showToast('기준 프리셋을 선택해주세요.', '⚙');
            return;
        }
        elements.savePresetBtn.disabled = true;
        try {
            const response = isEdit
                ? await api.updateEnginePreset(state.editingUserPresetId, payload)
                : await api.createEnginePreset(payload);
            if (response.ok) {
                const result = isEdit ? { id: state.editingUserPresetId } : await response.json();
                closePresetCreatePanel();
                await fetchEnginePresets(result.id);
                elements.researchEnginePresetSelect.value = String(result.id);
                syncPresetStatusBadge();
                actions.showToast(isEdit ? '프리셋 수정됨' : '프리셋 생성됨', '⚙');
            } else if (response.status === 409) {
                actions.showToast('같은 이름의 프리셋이 있습니다.', '❌');
            } else {
                actions.showToast('프리셋을 생성하지 못했습니다.', '❌');
            }
        } finally {
            elements.savePresetBtn.disabled = false;
        }
    }

    async function deleteSelectedPreset() {
        const preset = state.enginePresets.find(item => Number(item.id) === Number(state.editingPresetId));
        if (!preset || Number(preset.id) <= 0) return;
        if (!confirm(`'${preset.name}' 프리셋을 삭제할까요?`)) return;
        const response = await api.deleteEnginePreset(preset.id);
        if (response.ok) {
            closePresetCreatePanel();
            const fallback = state.enginePresets.find(item => Number(item.id) < 0);
            const fallbackId = fallback ? fallback.id : null;
            await fetchEnginePresets(fallbackId);
            if (fallbackId) elements.researchEnginePresetSelect.value = String(fallbackId);
            syncPresetStatusBadge();
            actions.showToast('프리셋 삭제됨', '⚙');
        } else {
            actions.showToast('프리셋을 삭제하지 못했습니다.', '❌');
        }
    }

    async function testSelectedPreset() {
        if (!state.editingPresetId) {
            actions.showToast('프리셋을 선택해주세요.', '⚙');
            return;
        }
        elements.testPresetBtn.disabled = true;
        try {
            const result = await testPresetById(state.editingPresetId);
            if (result) actions.showToast('프리셋 테스트 완료', '⚙');
        } finally {
            elements.testPresetBtn.disabled = false;
        }
    }

    async function testPresetById(id) {
        const response = await api.testEnginePreset(id);
        if (!response.ok) return null;
        const result = await response.json();
        const preset = state.enginePresets.find(item => Number(item.id) === Number(id));
        if (preset) {
            preset.last_test_status = result.status || 'unverified';
            preset.last_test_message = result.message || '';
        }
        if (String(state.editingPresetId) === String(id)) {
            elements.presetTestStatus.className = `preset-status-badge status-${result.status || 'unverified'}`;
            elements.presetTestStatus.textContent = presetStatusLabel(result.status);
            elements.presetTestMessage.textContent = result.message || '';
        }
        syncPresetStatusBadge();
        return result;
    }

    return {
        fetchEnginePresets,
        fetchModels,
        getSelectedEnginePreset,
        syncPresetStatusBadge,
        openPresetModal,
        loadPresetIntoEditor,
        closePresetCreatePanel,
        startNewPresetFromSelected,
        startEditUserPreset,
        deleteSelectedPreset,
        savePreset,
        testSelectedPreset
    };
}
