import { copyTextToClipboard } from '../utils.js';
import { closeModal, openModal } from './modals.js';

function currentFile(state, filename) {
    return state.allFiles.find(file => file.filename === filename) || null;
}

export function updateViewerTitle(context) {
    const { state, elements } = context;
    const file = state.currentView === 'viewer' && state.currentFilename
        ? currentFile(state, state.currentFilename)
        : null;
    const title = file ? file.original_name || '' : '';

    if (elements.viewerTitleStrip) {
        elements.viewerTitleStrip.classList.toggle('hidden', !title);
        elements.viewerTitleStrip.classList.toggle('is-clickable-title', Boolean(title));
    }
    if (elements.viewerTitleStrip) {
        if (title) {
            elements.viewerTitleStrip.setAttribute('role', 'button');
            elements.viewerTitleStrip.setAttribute('tabindex', '0');
            elements.viewerTitleStrip.setAttribute('aria-label', '조사 요청 내용 확인');
            elements.viewerTitleStrip.title = '조사 요청 내용 확인';
        } else {
            elements.viewerTitleStrip.removeAttribute('role');
            elements.viewerTitleStrip.removeAttribute('tabindex');
            elements.viewerTitleStrip.removeAttribute('aria-label');
            elements.viewerTitleStrip.removeAttribute('title');
        }
    }
    if (elements.viewerTitleText) {
        elements.viewerTitleText.textContent = title;
        elements.viewerTitleText.title = title;
        elements.viewerTitleText.classList.remove('is-overflowing');
        requestAnimationFrame(() => {
            if (!elements.viewerTitleText || (elements.viewerTitleStrip && elements.viewerTitleStrip.classList.contains('hidden'))) return;
            const hasOverflow = elements.viewerTitleText.scrollWidth > elements.viewerTitleStrip.clientWidth;
            elements.viewerTitleText.classList.toggle('is-overflowing', hasOverflow);
        });
    }
    if (elements.relationshipGraphBtn) {
        elements.relationshipGraphBtn.classList.toggle('hidden', !state.currentFilename || state.currentView !== 'viewer');
    }
}

export async function showResearchRequest(context, actions) {
    const { state, elements, api } = context;
    if (!state.currentFilename) return;

    try {
        const cacheKey = state.currentFilename;
        const info = state.researchRequestCache.get(cacheKey)
            || await api.getResearchRequest(state.currentFilename);
        state.researchRequestCache.set(cacheKey, info);
        const file = currentFile(state, state.currentFilename);
        if (file) file.has_research_request = true;
        renderResearchRequestModal(context, actions, info);
        openModal(elements.researchRequestModal);
        updateViewerTitle(context);
        actions.resetActivityTimer();
    } catch (err) {
        console.error('Failed to load research request:', err);
        actions.showToast('연결된 조사 요청을 찾지 못했습니다.', 'ℹ️');
    }
}

export async function showRelationshipGraph(context, actions) {
    const { state, elements, api } = context;
    if (!state.currentFilename) return;

    const filename = state.currentFilename;
    try {
        renderRelationshipGraphLoading(context);
        openModal(elements.relationshipGraphModal);
        const graph = state.relationshipGraphCache.get(filename)
            || await api.getRelationshipGraph(filename, { depth: 1, direction: 'both' });
        state.relationshipGraphCache.set(filename, graph);
        if (state.currentFilename !== filename) return;
        renderRelationshipGraphModal(context, actions, graph || {});
        actions.resetActivityTimer();
    } catch (err) {
        console.error('Failed to load document relationships:', err);
        closeModal(elements.relationshipGraphModal);
        actions.showToast('문서 관계를 불러오지 못했습니다.', '⚠');
    }
}

function renderResearchRequestModal(context, actions, info) {
    const { state, elements } = context;
    if (!elements.researchRequestModal) return;

    const title = currentFileTitle(state, state.currentFilename) || info.original_name || '';
    state.researchRequestTitleToCopy = title;
    if (elements.researchRequestDocumentTitle) {
        elements.researchRequestDocumentTitle.textContent = title;
        elements.researchRequestDocumentTitle.title = title;
    }
    replaceMeta(elements.researchRequestMeta, buildMetaItems(info));
    setTextSection(
        elements.researchRequestTopicSection,
        elements.researchRequestTopic,
        info.research_topic
    );
    setTextSection(
        elements.researchRequestInstructionsSection,
        elements.researchRequestInstructions,
        info.research_instructions
    );
    setRelationshipList(
        elements,
        actions,
        info.relationships || {},
        info.source_documents || [],
        info.source_filenames || []
    );
    setPromptSection(elements, info.request_prompt);
    setTextSection(
        elements.researchRequestFailureSection,
        elements.researchRequestFailure,
        info.quality_last_failure
    );
}

function currentFileTitle(state, filename) {
    const file = currentFile(state, filename);
    return file ? file.original_name || '' : '';
}

function renderRelationshipGraphLoading(context) {
    const { state, elements } = context;
    if (elements.relationshipGraphTitle) {
        elements.relationshipGraphTitle.textContent = currentFileTitle(state, state.currentFilename) || state.currentFilename || '';
    }
    if (elements.relationshipGraphBody) {
        elements.relationshipGraphBody.replaceChildren();
        const loading = document.createElement('div');
        loading.className = 'relationship-graph-empty';
        loading.textContent = '불러오는 중...';
        elements.relationshipGraphBody.appendChild(loading);
    }
}

function renderRelationshipGraphModal(context, actions, relationships) {
    const { state, elements } = context;
    if (!elements.relationshipGraphBody) return;

    const relationshipGraph = normalizeRelationshipGraph(relationships);
    const sources = graphLinksForDirection(relationshipGraph, 'sources');
    const derivatives = graphLinksForDirection(relationshipGraph, 'derivatives');
    const title = relationshipGraph.root
        ? relationshipGraph.root.title || relationshipGraph.root.filename
        : currentFileTitle(state, state.currentFilename) || state.currentFilename || '';

    if (elements.relationshipGraphTitle) {
        elements.relationshipGraphTitle.textContent = title;
        elements.relationshipGraphTitle.title = title;
    }

    elements.relationshipGraphBody.replaceChildren();
    if (sources.length === 0 && derivatives.length === 0) {
        const empty = document.createElement('div');
        empty.className = 'relationship-graph-empty';
        empty.textContent = '연결된 문서가 없습니다.';
        elements.relationshipGraphBody.appendChild(empty);
        return;
    }

    const graph = document.createElement('div');
    graph.className = 'relationship-graph';
    const svg = buildRelationshipEdges(sources, derivatives);
    graph.appendChild(svg);
    graph.appendChild(buildGraphColumn(context, actions, 'sources', '기반 문서', sources));
    graph.appendChild(buildCurrentGraphNode(title, state.currentFilename));
    graph.appendChild(buildGraphColumn(context, actions, 'derivatives', '파생 문서', derivatives));
    elements.relationshipGraphBody.appendChild(graph);
}

function normalizeRelationshipGraph(value) {
    const nodes = Array.isArray(value.nodes) ? value.nodes : [];
    const edges = Array.isArray(value.edges) ? value.edges : [];
    const root = value.root || null;
    const nodeById = new Map(nodes.map(node => [node.id, node]));
    return { root, nodeById, edges };
}

function graphLinksForDirection(graph, direction) {
    if (!graph.root) return [];
    const rootId = graph.root.id;
    return graph.edges
        .map(edge => {
            const linkedId = direction === 'sources'
                ? edge.to_file_id
                : edge.from_file_id;
            const isMatch = direction === 'sources'
                ? edge.from_file_id === rootId
                : edge.to_file_id === rootId;
            const document = isMatch ? graph.nodeById.get(linkedId) : null;
            return document ? {
                id: edge.id,
                from_file_id: edge.from_file_id,
                to_file_id: edge.to_file_id,
                relation_type: edge.relation_type,
                created_by_task_id: edge.created_by_task_id,
                created_at: edge.created_at,
                document
            } : null;
        })
        .filter(Boolean);
}

function buildGraphColumn(context, actions, kind, label, links) {
    const column = document.createElement('div');
    column.className = `relationship-graph-column relationship-graph-${kind}`;
    const heading = document.createElement('div');
    heading.className = 'relationship-graph-column-heading';
    heading.textContent = label;
    column.appendChild(heading);

    const stack = document.createElement('div');
    stack.className = 'relationship-graph-stack';
    if (links.length === 0) {
        const empty = document.createElement('div');
        empty.className = 'relationship-graph-node relationship-graph-node-empty';
        empty.textContent = '없음';
        stack.appendChild(empty);
    } else {
        for (const link of links) {
            stack.appendChild(buildLinkedGraphNode(context, actions, link));
        }
    }
    column.appendChild(stack);
    return column;
}

function buildCurrentGraphNode(title, filename) {
    const column = document.createElement('div');
    column.className = 'relationship-graph-current';
    const node = document.createElement('div');
    node.className = 'relationship-graph-node relationship-graph-node-current';
    node.title = filename || title;

    const type = document.createElement('span');
    type.className = 'relationship-graph-node-type';
    type.textContent = '현재 문서';
    const name = document.createElement('span');
    name.className = 'relationship-graph-node-name';
    name.textContent = title || filename || '현재 문서';
    node.append(type, name);
    column.appendChild(node);
    return column;
}

function buildLinkedGraphNode(context, actions, relation) {
    const { elements } = context;
    const button = document.createElement('button');
    button.className = 'relationship-graph-node relationship-graph-node-link';
    button.type = 'button';
    button.title = relation.document.filename;
    button.addEventListener('click', () => {
        closeModal(elements.relationshipGraphModal);
        actions.loadContent(relation.document.filename);
    });

    const type = document.createElement('span');
    type.className = 'relationship-graph-node-type';
    type.textContent = relationTypeLabel(relation.relation_type);
    const name = document.createElement('span');
    name.className = 'relationship-graph-node-name';
    name.textContent = relation.document.title || relation.document.filename;
    button.append(type, name);
    return button;
}

function buildRelationshipEdges(sources, derivatives) {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('class', 'relationship-graph-edges');
    svg.setAttribute('viewBox', '0 0 100 100');
    svg.setAttribute('preserveAspectRatio', 'none');
    svg.setAttribute('aria-hidden', 'true');
    appendRelationshipEdgeSet(svg, sources, 'source');
    appendRelationshipEdgeSet(svg, derivatives, 'derivative');
    return svg;
}

function appendRelationshipEdgeSet(svg, links, direction) {
    const x1 = direction === 'source' ? 18 : 50;
    const x2 = direction === 'source' ? 50 : 82;
    const labelX = direction === 'source' ? 33 : 67;
    const targetY = 50;
    links.forEach((link, index) => {
        const rowY = graphRowY(index, links.length);
        const y1 = direction === 'source' ? rowY : targetY;
        const y2 = direction === 'source' ? targetY : rowY;
        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('class', 'relationship-graph-edge');
        path.setAttribute('d', `M ${x1} ${y1} C ${labelX} ${y1}, ${labelX} ${y2}, ${x2} ${y2}`);
        svg.appendChild(path);

        const label = document.createElementNS('http://www.w3.org/2000/svg', 'text');
        label.setAttribute('class', 'relationship-graph-edge-label');
        label.setAttribute('x', String(labelX));
        label.setAttribute('y', String((y1 + y2) / 2 - 2));
        label.textContent = relationTypeLabel(link.relation_type);
        svg.appendChild(label);
    });
}

function graphRowY(index, count) {
    if (count <= 1) return 50;
    const min = 18;
    const max = 82;
    return min + ((max - min) * index / (count - 1));
}

export async function copyResearchRequestTitle(context, actions) {
    const { state } = context;
    if (!state.researchRequestTitleToCopy) return;
    try {
        await copyTextToClipboard(state.researchRequestTitleToCopy);
        actions.showToast('제목이 클립보드에 복사되었습니다.', '📋');
    } catch (err) {
        console.error('Failed to copy research request title:', err);
        actions.showToast('제목 복사 실패', '❌');
    }
}

function buildMetaItems(info) {
    const items = [
        ['작업 ID', info.task_id],
        ['등록 시각', formatDateTime(info.created_at)],
        ['조사 유형', formatType(info.research_type)],
        ['조사 모드', formatType(info.research_mode)],
        ['보고서 형식', formatType(info.research_format)],
        ['엔진', info.engine_preset_name || info.model || info.resolved_model],
        ['엔진 종류', formatType(info.engine_kind)],
        ['조사 강도', formatType(info.research_intensity)],
        ['검증 반복', info.quality_max_iterations ? `최대 ${info.quality_max_iterations}회` : null],
        ['검증 깊이', formatType(info.quality_depth)],
        ['신뢰도 상태', formatType(info.quality_status)],
        ['웹 검색', formatWebSearch(info)]
    ];
    return items.filter(([, value]) => value !== null && value !== undefined && value !== '');
}

function replaceMeta(container, items) {
    if (!container) return;
    container.replaceChildren();
    for (const [label, value] of items) {
        const row = document.createElement('div');
        row.className = 'research-request-meta-row';
        const key = document.createElement('span');
        key.className = 'research-request-meta-label';
        key.textContent = label;
        const val = document.createElement('span');
        val.className = 'research-request-meta-value';
        val.textContent = String(value);
        row.append(key, val);
        container.appendChild(row);
    }
}

function setTextSection(section, target, value) {
    const text = typeof value === 'string' ? value.trim() : '';
    if (section) section.classList.toggle('hidden', !text);
    if (target) target.textContent = text;
}

function setRelationshipList(elements, actions, relationships, sourceDocuments, sourceFilenames) {
    const list = elements.researchRequestSources;
    const section = elements.researchRequestSourcesSection;
    if (!list || !section) return;
    list.replaceChildren();

    const sources = Array.isArray(relationships.sources) ? relationships.sources : [];
    const derivatives = Array.isArray(relationships.derivatives) ? relationships.derivatives : [];
    const linkedSourceFilenames = new Set();

    if (sources.length > 0) {
        appendRelationshipGroup(list, elements, actions, '기반 문서', sources, linkedSourceFilenames);
    }
    appendFallbackSourceGroup(list, elements, actions, sourceDocuments, sourceFilenames, linkedSourceFilenames);

    if (derivatives.length > 0) {
        appendRelationshipGroup(list, elements, actions, '파생 문서', derivatives, new Set());
    }

    section.classList.toggle('hidden', list.children.length === 0);
}

function appendRelationshipGroup(list, elements, actions, title, links, linkedFilenames) {
    const group = document.createElement('li');
    group.className = 'research-request-relationship-group';
    const heading = document.createElement('div');
    heading.className = 'research-request-relationship-heading';
    heading.textContent = title;
    group.appendChild(heading);

    for (const relation of links) {
        if (!relation || !relation.document || !relation.document.filename) continue;
        const row = document.createElement('a');
        row.className = 'research-request-relationship-row';
        row.href = `#/viewer/${encodeURIComponent(relation.document.filename)}`;
        row.title = relation.document.filename;
        row.addEventListener('click', (event) => {
            event.preventDefault();
            closeModal(elements.researchRequestModal);
            actions.loadContent(relation.document.filename);
        });

        const type = document.createElement('span');
        type.className = 'research-request-relationship-type';
        type.textContent = relationTypeLabel(relation.relation_type);

        const name = document.createElement('span');
        name.className = 'research-request-relationship-name';
        name.textContent = relation.document.title || relation.document.filename;

        row.append(type, name);
        group.appendChild(row);
        linkedFilenames.add(relation.document.filename);
    }

    if (group.children.length > 1) list.appendChild(group);
}

function appendFallbackSourceGroup(list, elements, actions, sourceDocuments, sourceFilenames, linkedFilenames) {
    const group = document.createElement('li');
    group.className = 'research-request-relationship-group';
    const heading = document.createElement('div');
    heading.className = 'research-request-relationship-heading';
    heading.textContent = '기반 문서';
    group.appendChild(heading);

    for (const source of sourceDocuments) {
        if (!source || !source.filename || linkedFilenames.has(source.filename)) continue;
        const link = document.createElement('a');
        link.className = 'research-request-relationship-row';
        link.href = `#/viewer/${encodeURIComponent(source.filename)}`;
        link.title = source.filename;
        link.addEventListener('click', (event) => {
            event.preventDefault();
            closeModal(elements.researchRequestModal);
            actions.loadContent(source.filename);
        });

        const type = document.createElement('span');
        type.className = 'research-request-relationship-type';
        type.textContent = 'Source';

        const name = document.createElement('span');
        name.className = 'research-request-relationship-name';
        name.textContent = source.title || source.filename;

        link.append(type, name);
        group.appendChild(link);
        linkedFilenames.add(source.filename);
    }

    for (const source of sourceFilenames) {
        if (!source || linkedFilenames.has(source)) continue;
        const item = document.createElement('div');
        item.className = 'research-request-source-missing';
        item.textContent = source;
        group.appendChild(item);
    }

    if (group.children.length > 1) list.appendChild(group);
}

function setPromptSection(elements, prompt) {
    const text = typeof prompt === 'string' ? prompt.trim() : '';
    if (elements.researchRequestPromptSection) {
        elements.researchRequestPromptSection.classList.toggle('hidden', !text);
    }
    if (elements.researchRequestPromptSection) {
        elements.researchRequestPromptSection.open = false;
    }
    if (elements.researchRequestPrompt) {
        elements.researchRequestPrompt.textContent = text;
    }
}

function formatDateTime(value) {
    if (!value) return null;
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    try {
        return new Intl.DateTimeFormat('ko-KR', {
            year: 'numeric',
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit'
        }).format(date);
    } catch (err) {
        return date.toLocaleString();
    }
}

function formatType(value) {
    if (!value) return null;
    const labels = {
        initial: '새 주제',
        deep: '심층 조사',
        follow_up: '후속 조사',
        synthesis: '융합 조사',
        general: '일반',
        local: '로컬/장소 탐색',
        historical: '역사 조사',
        technology: '기술 구현',
        technology_concept: '기술 개념',
        technology_implementation: '기술 구현',
        science: '과학/연구',
        business: '비즈니스/시장',
        policy: '정책/규제',
        culture: '문화/사회',
        music: '음악/아티스트',
        md: '마크다운',
        html: '인터랙티브 HTML',
        low: '낮음',
        medium: '보통',
        high: '높음',
        light: '가벼움',
        standard: '표준',
        strict: '엄격',
        trusted: '신뢰',
        untrusted: '신뢰도 없음',
        cli: 'CLI',
        pi_ollama: 'Pi/Ollama'
    };
    return labels[value] || value;
}

function formatWebSearch(info) {
    if (!info.web_search_requested) return null;
    const requested = info.web_search_requested === 'true' ? '요청됨' : '요청 안 함';
    return info.web_search_provider ? `${requested} · ${info.web_search_provider}` : requested;
}

function relationTypeLabel(value) {
    const labels = {
        derived_from: 'Derived from',
        translated_from: 'Translated from',
        followed_up_from: 'Follow-up from',
        synthesized_from: 'Synthesized from'
    };
    return labels[value] || 'Related source';
}

export function renderViewerContent(context, filename, actions, push = true) {
    const { state, elements } = context;
    state.currentFilename = filename;
    updateViewerTitle(context);
    elements.contentViewer.innerHTML = '<div class="placeholder"><p>로딩 중...</p></div>';
    elements.viewerFloatingMenu.classList.add('hidden');

    if (push) {
        actions.switchView('viewer', filename, true);
    }

    const url = `/api/files/${encodeURIComponent(filename)}/content?t=${Date.now()}`;
    elements.contentViewer.innerHTML = '';
    const iframe = document.createElement('iframe');
    iframe.style.width = '100%';
    iframe.style.height = '100%';
    iframe.style.border = 'none';
    iframe.style.background = 'transparent';
    iframe.src = url;

    iframe.onload = () => {
        elements.viewerFloatingMenu.classList.remove('hidden');
        try {
            const doc = iframe.contentWindow.document;
            const events = ['mousemove', 'mousedown', 'keydown', 'touchstart', 'scroll'];
            events.forEach(evt => {
                doc.addEventListener(evt, actions.resetActivityTimer, true);
            });
        } catch (e) {
            console.warn('Cannot attach listeners to iframe due to cross-origin or other restrictions');
        }
    };

    elements.contentViewer.appendChild(iframe);
    elements.contentViewer.appendChild(elements.viewerFloatingMenu);
    if (state.currentView === 'library') actions.renderLibraryGrid();
}

export function scrollViewerTo(context, position, actions) {
    const { elements } = context;
    const iframe = elements.contentViewer.querySelector('iframe');
    if (!iframe || !iframe.contentWindow) return;

    try {
        const doc = iframe.contentWindow.document;
        const top = position === 'bottom'
            ? Math.max(
                doc.body ? doc.body.scrollHeight || 0 : 0,
                doc.documentElement ? doc.documentElement.scrollHeight || 0 : 0
            )
            : 0;

        iframe.contentWindow.scrollTo({ top, behavior: 'auto' });
        actions.resetActivityTimer();
    } catch (err) {
        console.warn('Failed to scroll viewer iframe: ', err);
        actions.showToast('이동 실패', '❌');
    }
}

export async function copyViewerContent(context, actions) {
    const { state, api } = context;
    if (!state.currentFilename) return;
    try {
        const response = await api.getRawFileContent(state.currentFilename);
        if (!response.ok) throw new Error('Failed to load raw content');
        const content = await response.text();
        await copyTextToClipboard(content);
        actions.showToast('원본 내용이 클립보드에 복사되었습니다.', '📋');
    } catch (err) {
        console.error('Failed to copy: ', err);
        actions.showToast('복사 실패', '❌');
    }
}
