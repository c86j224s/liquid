export function initScrapingController(context, actions) {
    const { api, elements } = context;

    function openScrapModal() {
        elements.scrapUrlInput.value = '';
        elements.scrapModeSelect.value = 'general';
        elements.scrapReferencesInput.value = '';
        elements.scrapModal.classList.remove('hidden');
        elements.scrapUrlInput.focus();
    }

    function getScrapReferences() {
        return elements.scrapReferencesInput.value
            .split('\n')
            .map(line => line.trim())
            .filter(Boolean);
    }

    async function scrapUrl() {
        const url = elements.scrapUrlInput.value.trim();
        if (!url) return;
        const mode = elements.scrapModeSelect.value;
        const references = getScrapReferences();
        elements.scrapBtn.disabled = true;
        try {
            const response = await api.scrapUrl({ url, references, mode });
            if (response.ok) {
                elements.scrapModal.classList.add('hidden');
                actions.fetchTasks();
                actions.showToast('스크랩 작업이 시작되었습니다.', '📄');
            }
        } finally {
            elements.scrapBtn.disabled = false;
        }
    }

    async function scrapUrlWithTranslation() {
        const url = elements.scrapUrlInput.value.trim();
        if (!url) return;
        const model = elements.translateModelSelect.value;
        const mode = elements.scrapModeSelect.value;
        const references = getScrapReferences();
        elements.scrapTranslateBtn.disabled = true;
        try {
            const response = await api.scrapUrl({ url, references, translate: true, model, mode });
            if (response.ok) {
                elements.scrapModal.classList.add('hidden');
                actions.fetchTasks();
                actions.showToast('스크랩 및 번역 작업이 시작되었습니다.', '🌐');
            }
        } finally {
            elements.scrapTranslateBtn.disabled = false;
        }
    }

    async function handleFileUpload(event) {
        const file = event.target.files[0];
        if (!file) return;
        const data = new FormData();
        data.append('file', file);
        const response = await api.uploadFile(data);
        if (response.ok) {
            actions.refreshLibraryData();
            actions.showToast('업로드 완료', '📤');
        }
    }

    return { openScrapModal, scrapUrl, scrapUrlWithTranslation, handleFileUpload };
}
