export function initResearchController(context, actions) {
    const { state, elements, api } = context;

    function syncQualityDefaultsFromIntensity({ force = false } = {}) {
        if (state.researchQualityManuallyChanged && !force) return;
        const isHighIntensity = elements.researchIntensitySelect.value === 'high';
        if (isHighIntensity) {
            elements.researchQualityIterationsSelect.value = '2';
            elements.researchQualityDepthSelect.value = 'strict';
        } else {
            elements.researchQualityIterationsSelect.value = '1';
            elements.researchQualityDepthSelect.value = 'standard';
        }
    }

    function openResearchModal(filename, title, researchType = 'deep') {
        state.researchType = researchType;
        state.isTopicResearch = researchType === 'initial';
        const defaultInstructionPlaceholder = '예: 비교 대상, 제외 조건, 예산/기간/지역, 검증할 주장, 후보별 장단점과 불확실성';
        if (state.isTopicResearch) {
            elements.researchModalTitle.textContent = '최초 주제 조사';
            elements.topicInputContainer.classList.remove('hidden');
            elements.researchTargetText.classList.add('hidden');
            state.researchTargets = [];
            elements.researchTopicInput.value = '';
            elements.researchInstruction.placeholder = defaultInstructionPlaceholder;
        } else {
            elements.researchModalTitle.textContent = researchType === 'follow_up' ? '후속 조사 질문' : 'AI 심층 조사 설정';
            elements.topicInputContainer.classList.add('hidden');
            elements.researchTargetText.classList.remove('hidden');
            if (!filename && state.selectedFiles.size > 0) {
                state.researchType = 'synthesis';
                state.researchTargets = Array.from(state.selectedFiles);
                elements.researchModalTitle.textContent = '문서 융합 조사';
                elements.researchTargetText.textContent = `${state.selectedFiles.size}개의 문서를 융합하여 조사합니다.`;
                elements.researchInstruction.placeholder = '예: 문서 간 공통점/차이, 충돌하는 주장, 통합 결론, 추가 확인할 쟁점';
            } else {
                state.researchTargets = [filename];
                elements.researchTargetText.textContent = researchType === 'follow_up'
                    ? `'${title}' 문서에 대해 후속 질문을 조사합니다.`
                    : `'${title}' 문서를 심층 조사합니다.`;
                elements.researchInstruction.placeholder = researchType === 'follow_up'
                    ? '예: 이전 결과에서 더 확인할 질문, 반론, 최신 정보, 빠진 후보나 근거'
                    : defaultInstructionPlaceholder;
            }
        }
        elements.researchInstruction.value = '';
        elements.researchModeSelect.value = 'general';
        const preset = actions.getSelectedEnginePreset();
        if (preset && preset.default_intensity) elements.researchIntensitySelect.value = preset.default_intensity;
        state.researchQualityManuallyChanged = false;
        syncQualityDefaultsFromIntensity({ force: true });
        actions.syncPresetStatusBadge();
        elements.researchModal.classList.remove('hidden');
    }

    async function startResearch() {
        const format = elements.researchFormatSelect.value;
        const enginePresetId = Number(elements.researchEnginePresetSelect.value);
        if (!enginePresetId) {
            alert('엔진 프리셋을 선택해주세요.');
            return;
        }
        const mode = elements.researchModeSelect.value;
        const researchIntensity = elements.researchIntensitySelect.value;
        const researchQualityMaxIterations = Number(elements.researchQualityIterationsSelect.value) || 1;
        const researchQualityDepth = elements.researchQualityDepthSelect.value;
        const instruction = elements.researchInstruction.value.trim();
        let response;
        if (state.isTopicResearch) {
            const topic = elements.researchTopicInput.value.trim();
            if (!topic) {
                alert('주제를 입력해주세요.');
                return;
            }
            response = await api.startTopicResearch({
                topic,
                engine_preset_id: enginePresetId,
                research_intensity: researchIntensity,
                research_quality_max_iterations: researchQualityMaxIterations,
                research_quality_depth: researchQualityDepth,
                instructions: instruction || null,
                mode,
                format,
                research_type: state.researchType
            });
        } else {
            response = await api.startFileResearch({
                filenames: state.researchTargets,
                engine_preset_id: enginePresetId,
                research_intensity: researchIntensity,
                research_quality_max_iterations: researchQualityMaxIterations,
                research_quality_depth: researchQualityDepth,
                instructions: instruction || null,
                mode,
                format,
                research_type: state.researchType
            });
        }
        if (response.status === 202) {
            elements.researchModal.classList.add('hidden');
            actions.clearSelection();
            actions.showToast('조사 작업이 시작되었습니다.', '🔍');
        }
    }

    async function startTranslate() {
        const model = elements.translateFileModelSelect.value;
        if (!model) {
            alert('모델을 선택해주세요.');
            return;
        }
        const response = await api.translateFile(state.translatingFilename, { model });
        if (response.status === 202) {
            elements.translateModal.classList.add('hidden');
            actions.showToast('번역 작업이 시작되었습니다.', '🌐');
        }
    }

    return { openResearchModal, startResearch, startTranslate, syncQualityDefaultsFromIntensity };
}
