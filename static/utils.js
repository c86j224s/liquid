export function debounce(func, wait) {
    let timeout;
    return function executedFunction(...args) {
        const later = () => {
            clearTimeout(timeout);
            func(...args);
        };
        clearTimeout(timeout);
        timeout = setTimeout(later, wait);
    };
}

export function normalizeFileStatus(status) {
    return status === 'draft' || status === 'published' || status === 'archived' ? status : 'unknown';
}

export function normalizeFileType(fileType) {
    return fileType === 'md' ? 'md' : 'html';
}

export function escapeHtml(value) {
    return String(value == null ? '' : value).replace(/[&<>"']/g, (char) => ({
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#39;'
    }[char]));
}

export function getStatusTransition(status) {
    const normalized = normalizeFileStatus(status);
    if (normalized === 'draft') {
        return {
            nextStatus: 'published',
            title: '클릭하면 Published로 전환됩니다.',
            ariaLabel: '현재 Draft 상태입니다. 클릭하면 Published로 전환됩니다.',
            successMessage: 'Published로 전환됨'
        };
    }
    if (normalized === 'published') {
        return {
            nextStatus: 'draft',
            title: '클릭하면 Draft로 전환됩니다.',
            ariaLabel: '현재 Published 상태입니다. 클릭하면 Draft로 전환됩니다.',
            successMessage: 'Draft로 전환됨'
        };
    }
    if (normalized === 'archived') {
        return {
            nextStatus: null,
            title: '보관 항목은 더보기 메뉴에서 Published로 복귀할 수 있습니다.',
            ariaLabel: 'Archived 상태입니다. 더보기 메뉴에서 Published로 복귀할 수 있습니다.',
            successMessage: ''
        };
    }
    return {
        nextStatus: null,
        title: '상태를 변경할 수 없습니다.',
        ariaLabel: '알 수 없는 상태입니다.',
        successMessage: ''
    };
}

export async function copyTextToClipboard(text) {
    if (navigator.clipboard && window.isSecureContext) {
        await navigator.clipboard.writeText(text);
        return;
    }

    const textarea = document.createElement('textarea');
    textarea.value = text;
    textarea.setAttribute('readonly', '');
    textarea.style.position = 'fixed';
    textarea.style.top = '-1000px';
    textarea.style.left = '-1000px';
    document.body.appendChild(textarea);
    textarea.focus();
    textarea.select();
    try {
        if (!document.execCommand('copy')) throw new Error('Copy command failed');
    } finally {
        textarea.remove();
    }
}
