export function initTasksController(context, actions) {
    const { api, elements, state } = context;
    const researchValueLabels = {
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
    };

    function setupTaskSSE() {
        const evtSource = new EventSource('/api/tasks/stream');
        evtSource.onmessage = async (event) => {
            const data = JSON.parse(event.data);
            console.log('Task update:', data);
            let refreshedTasks = null;

            if (data.status === 'completed') {
                const confidence = researchConfidenceLabel(data);
                const isLowConfidence = isLowConfidenceResearch(data);
                const suffix = confidence ? ` · ${confidence}` : '';
                actions.showToast(`'${data.original_name}' 작업 완료${suffix}`, isLowConfidence ? '⚠' : '🎉');
                actions.sendSystemNotification('작업 완료', `'${data.original_name}' 작업이 끝났습니다${suffix}.`);
                if (!isLowConfidence) actions.alertSound.play().catch(() => {});
                actions.refreshLibraryData();
            } else if (data.status === 'failed') {
                if (isLowConfidenceResearch(data)) {
                    actions.showToast(`'${data.original_name}' 작업 완료 · 신뢰도 낮음`, '⚠');
                    actions.refreshLibraryData();
                } else {
                    refreshedTasks = await fetchTasks();
                    const detailedTask = findTaskById(refreshedTasks, data.id) || data;
                    actions.showToast(
                        isScrapeTask(detailedTask)
                            ? `'${detailedTask.original_name || data.original_name}' 스크랩 실패`
                            : `'${detailedTask.original_name || data.original_name}' 기술적 실패`,
                        '❌'
                    );
                }
            }
            if (!refreshedTasks) await fetchTasks();
        };
        evtSource.onerror = () => {
            console.error('SSE connection failed. Reconnecting...');
        };
    }

    async function fetchTasks() {
        try {
            const tasks = await api.listTasks();
            state.tasks = tasks;
            renderTaskList(tasks);
            return tasks;
        } catch (e) { console.error(e); }
        return [];
    }

    function findTaskById(tasks, taskId) {
        if (!Array.isArray(tasks)) return null;
        return tasks.find(task => String(task.id) === String(taskId)) || null;
    }

    async function fetchConfig() {
        try {
            const config = await api.getConfig();
            const cloudWorkers = Number.isInteger(config.ai_workers) && config.ai_workers > 0 ? config.ai_workers : 1;
            const localWorkers = Number.isInteger(config.local_ai_workers) && config.local_ai_workers > 0 ? config.local_ai_workers : 1;
            elements.taskWorkerCount.textContent = `${cloudWorkers}/${localWorkers}`;
        } catch (e) { console.error(e); }
    }

    function renderTaskList(tasks) {
        if (!state.expandedResearchTaskIds) state.expandedResearchTaskIds = new Set();
        elements.taskList.innerHTML = '';
        if (tasks.length === 0) {
            elements.taskList.innerHTML = '<li class="task-item" style="color:rgba(255,255,255,0.3)">작업 없음</li>';
            return;
        }

        const statusMap = {
            'queued': '대기 중',
            'scraping': '스크랩 중',
            'translating': '번역 중',
            'researching': '심층 조사 중',
            'processing': '처리 중',
            'completed': '완료',
            'failed': '실패',
            'interrupted': '중단됨'
        };

        tasks.forEach(task => {
            const status = typeof task.status === 'string' ? task.status : '';
            const knownStatus = Object.prototype.hasOwnProperty.call(statusMap, status);
            const researchTask = isResearchTask(task);
            const taskId = String(task.id);
            const li = document.createElement('li');
            li.className = 'task-item';
            if (researchTask) li.classList.add('research-task-item');
            const displayStatus = taskStatusLabel(task, knownStatus ? statusMap[status] : status.toUpperCase());
            const qualityCurrent = Number(task.quality_current_iteration || 0);
            const qualityMax = Number(task.quality_max_iterations || 0);
            const qualityStatus = typeof task.quality_status === 'string' ? task.quality_status : '';
            const controllerStage = typeof task.research_controller_stage === 'string' ? task.research_controller_stage : '';
            const controllerIteration = Number(task.research_controller_iteration || 0);
            const controllerMax = Number(task.research_controller_max_iterations || 0);
            const isTerminal = ['completed', 'failed', 'interrupted'].includes(status);
            const controllerSuffix = !isTerminal && controllerStage
                ? ` · 연구 단계 ${controllerMax > 1 && controllerIteration > 0 ? `${controllerIteration}/${controllerMax} ` : ''}${researchStageLabel(controllerStage)}`
                : '';
            const qualitySuffix = !isTerminal && qualityMax > 1 && qualityCurrent > 0
                ? ` · 품질 패스 ${qualityCurrent}/${qualityMax}${qualityStatus ? ` ${qualityStatusLabel(qualityStatus)}` : ''}`
                : '';
            const canRetry = status === 'failed' || status === 'interrupted' || isLowConfidenceResearch(task);
            const canCancel = ['queued', 'scraping', 'translating', 'researching', 'processing'].includes(status);

            const row = document.createElement('div');
            row.className = 'task-row';
            const info = document.createElement('div');
            info.className = 'task-info';
            const name = document.createElement('span');
            name.className = 'task-name';
            name.title = task.original_name || '';
            name.textContent = task.original_name || '';
            info.appendChild(name);

            const statusEl = document.createElement('span');
            const statusClass = isLowConfidenceResearch(task) ? 'completed' : (knownStatus ? status : 'unknown');
            statusEl.className = `task-status status-${statusClass}`;
            statusEl.textContent = `${displayStatus}${controllerSuffix}${qualitySuffix}`;
            info.appendChild(statusEl);
            if (researchTask) {
                const meta = createResearchMetaLine(task);
                if (meta) info.appendChild(meta);
            }

            const taskActions = document.createElement('div');
            taskActions.className = 'task-actions';
            if (canRetry) {
                const retry = document.createElement('span');
                retry.className = 'task-action retry-task';
                retry.title = '재시도';
                retry.textContent = '🔄';
                taskActions.appendChild(retry);
            }

            const deleteAction = document.createElement('span');
            deleteAction.className = 'task-action delete-task';
            deleteAction.title = canCancel ? '취소' : '삭제';
            deleteAction.textContent = canCancel ? '⏹' : '✕';
            taskActions.appendChild(deleteAction);

            row.appendChild(info);
            row.appendChild(taskActions);
            li.appendChild(row);
            if (researchTask) {
                const details = createResearchTaskDetails(task);
                details.classList.toggle('hidden', !state.expandedResearchTaskIds.has(taskId));
                li.appendChild(details);
            }
            li.addEventListener('click', (e) => {
                if (e.target.classList.contains('task-action')) return;
                if (researchTask) {
                    if (state.expandedResearchTaskIds.has(taskId)) {
                        state.expandedResearchTaskIds.delete(taskId);
                    } else {
                        state.expandedResearchTaskIds.add(taskId);
                    }
                    renderTaskList(tasks);
                    return;
                }
                if (status === 'failed' && task.error_message && !isLowConfidenceResearch(task)) {
                    actions.showErrorMessage(task.error_message);
                    return;
                }
                if (isLowConfidenceResearch(task) && task.quality_last_failure) {
                    actions.showErrorMessage(task.quality_last_failure);
                }
            });
            deleteAction.addEventListener('click', async (e) => {
                e.stopPropagation();
                await api.deleteTask(task.id);
                if (canCancel) actions.showToast('작업 취소됨', '⏹');
                fetchTasks();
            });
            if (canRetry) {
                li.querySelector('.retry-task').addEventListener('click', async (e) => {
                    e.stopPropagation();
                    if (isResearchTask(task)) {
                        openRetryModal(task);
                    } else {
                        const response = await api.retryTask(task.id, { derive_task: false });
                        if (response.ok) {
                            actions.showToast('작업 재시도 시작', '🔄');
                            fetchTasks();
                        } else {
                            actions.showToast('재시도 실패', '❌');
                        }
                    }
                });
            }
            elements.taskList.appendChild(li);
        });
    }

    function createResearchMetaLine(task) {
        const pieces = [];
        const stage = typeof task.research_controller_stage === 'string' ? task.research_controller_stage : '';
        const controllerIteration = Number(task.research_controller_iteration || 0);
        const controllerMax = Number(task.research_controller_max_iterations || 0);
        const qualityCurrent = Number(task.quality_current_iteration || 0);
        const qualityMax = Number(task.quality_max_iterations || 0);
        const qualityStatus = typeof task.quality_status === 'string' ? task.quality_status : '';
        if (stage) pieces.push(`${researchStageLabel(stage)}${formatProgress(controllerIteration, controllerMax)}`);
        if (qualityMax > 0) pieces.push(`품질${formatProgress(qualityCurrent, qualityMax)}`);
        if (qualityStatus) pieces.push(qualityStatusLabel(qualityStatus));
        if (task.quality_depth) pieces.push(String(task.quality_depth));
        if (!pieces.length) return null;

        const meta = document.createElement('span');
        meta.className = 'task-research-meta';
        meta.textContent = pieces.join(' · ');
        return meta;
    }

    function createResearchTaskDetails(task) {
        const panel = document.createElement('div');
        panel.className = 'task-research-detail';

        const grid = document.createElement('div');
        grid.className = 'task-research-grid';
        appendDetailRow(grid, '현재 단계', researchStageSummary(task));
        appendDetailRow(grid, '신뢰도', researchConfidenceLabel(task) || '판단 전');
        appendDetailRow(grid, '품질 루프', qualityLoopSummary(task));
        appendDetailRow(grid, '엔진', engineSummary(task));
        appendDetailRow(grid, '조사 설정', researchConfigSummary(task));
        appendDetailRow(grid, '웹 검색', task.web_search_requested === 'true' ? (task.web_search_provider || '요청됨') : '요청 안 됨');
        panel.appendChild(grid);

        const failure = task.quality_last_failure || (task.status === 'failed' ? task.error_message : '');
        if (failure) {
            const failureBlock = document.createElement('div');
            failureBlock.className = 'task-research-failure';
            const label = document.createElement('strong');
            label.textContent = '품질 판단 사유';
            const body = document.createElement('pre');
            body.textContent = failure;
            failureBlock.appendChild(label);
            failureBlock.appendChild(body);
            panel.appendChild(failureBlock);
        }

        const sourceDiagnostics = parseSourceDiagnostics(task.research_source_diagnostics_json);
        if (sourceDiagnostics) {
            panel.appendChild(createSourceDiagnosticsBlock(sourceDiagnostics));
        }

        const events = parseResearchEvents(task.research_controller_artifacts_json);
        const timeline = document.createElement('div');
        timeline.className = 'task-research-timeline';
        const title = document.createElement('strong');
        title.textContent = '최근 연구 단계 기록';
        timeline.appendChild(title);
        if (events.length === 0) {
            const empty = document.createElement('p');
            empty.className = 'task-research-empty';
            empty.textContent = '아직 기록된 단계 이벤트가 없습니다.';
            timeline.appendChild(empty);
        } else {
            events.slice(-8).forEach(event => timeline.appendChild(createResearchEventRow(event)));
        }
        panel.appendChild(timeline);

        return panel;
    }

    function createSourceDiagnosticsBlock(diagnostics) {
        const block = document.createElement('div');
        block.className = 'task-research-source-diagnostics';
        const label = document.createElement('strong');
        label.textContent = '웹 검색 진단';
        block.appendChild(label);

        const summary = document.createElement('p');
        summary.textContent = sourceDiagnosticsSummary(diagnostics);
        block.appendChild(summary);

        const queries = normalizeSourceQueries(diagnostics.queries).slice(0, 5);
        if (queries.length > 0) {
            const list = document.createElement('div');
            list.className = 'task-research-source-query-list';
            queries.forEach(query => {
                const row = document.createElement('div');
                row.className = `task-research-source-query query-${safeStatusClass(query.status || 'unknown')}`;
                const title = document.createElement('span');
                title.textContent = `${query.query || '-'} · ${sourceQueryStatusLabel(query.status)} · ${Number(query.result_count || 0)}건`;
                row.appendChild(title);
                if (query.error) {
                    const error = document.createElement('small');
                    error.textContent = query.error;
                    row.appendChild(error);
                }
                list.appendChild(row);
            });
            block.appendChild(list);
        }
        return block;
    }

    function parseSourceDiagnostics(raw) {
        if (!raw || typeof raw !== 'string') return null;
        try {
            const parsed = JSON.parse(raw);
            if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return null;
            return parsed;
        } catch (_) {
            return null;
        }
    }

    function normalizeSourceQueries(queries) {
        if (!Array.isArray(queries)) return [];
        return queries
            .filter(isSourceQueryObject)
            .map(normalizeSourceQuery);
    }

    function isSourceQueryObject(query) {
        return query !== null && typeof query === 'object' && !Array.isArray(query);
    }

    function normalizeSourceQuery(query) {
        return {
            query: typeof query.query === 'string' ? query.query : '',
            status: typeof query.status === 'string' ? query.status : '',
            result_count: Number(query.result_count || 0),
            error: typeof query.error === 'string' ? query.error : ''
        };
    }

    function sourceDiagnosticsSummary(diagnostics) {
        const status = sourceDiagnosticsStatusLabel(diagnostics.status);
        const subject = diagnostics.subject ? ` · ${diagnostics.subject}` : '';
        const adopted = Number(diagnostics.adopted_source_count || 0);
        const discovered = Number(diagnostics.discovered_source_count || 0);
        const seeded = Number(diagnostics.seeded_source_count || 0);
        const reason = diagnostics.reason ? ` · ${diagnostics.reason}` : '';
        return `${status}${subject} · 채택 ${adopted}건 / 검색 ${discovered}건 / 기본 ${seeded}건${reason}`;
    }

    function sourceDiagnosticsStatusLabel(status) {
        const labels = {
            success: '성공',
            partial: '부분 성공',
            empty: '빈 결과',
            error: '도구 실패',
            skipped: '건너뜀'
        };
        return labels[status] || status || '기록 없음';
    }

    function sourceQueryStatusLabel(status) {
        const labels = {
            success: '성공',
            empty: '빈 결과',
            error: '실패'
        };
        return labels[status] || status || '기록';
    }

    function appendDetailRow(parent, label, value) {
        const row = document.createElement('div');
        row.className = 'task-research-detail-row';
        const key = document.createElement('span');
        key.className = 'task-research-detail-label';
        key.textContent = label;
        const val = document.createElement('span');
        val.className = 'task-research-detail-value';
        val.textContent = value || '-';
        row.appendChild(key);
        row.appendChild(val);
        parent.appendChild(row);
    }

    function createResearchEventRow(event) {
        const row = document.createElement('div');
        const statusValue = typeof event.status === 'string' ? event.status : '';
        const stageValue = typeof event.stage === 'string' ? event.stage : '';
        const detailValue = typeof event.detail === 'string' ? event.detail : '';
        row.className = `task-research-event event-${safeStatusClass(statusValue || 'unknown')}`;
        const head = document.createElement('div');
        head.className = 'task-research-event-head';
        const stage = document.createElement('span');
        stage.textContent = `${researchStageLabel(stageValue)}${formatProgress(Number(event.iteration || 0), Number(event.max_iterations || 0))}`;
        const status = document.createElement('span');
        status.textContent = controllerStatusLabel(statusValue);
        head.appendChild(stage);
        head.appendChild(status);
        row.appendChild(head);
        if (detailValue) {
            const detail = document.createElement('p');
            detail.textContent = detailValue;
            row.appendChild(detail);
        }
        return row;
    }

    function parseResearchEvents(raw) {
        if (!raw || typeof raw !== 'string') return [];
        try {
            const parsed = JSON.parse(raw);
            if (!Array.isArray(parsed.events)) return [];
            return parsed.events
                .filter(isResearchEventObject)
                .map(normalizeResearchEvent);
        } catch (_) {
            return [];
        }
    }

    function isResearchEventObject(event) {
        return event !== null && typeof event === 'object' && !Array.isArray(event);
    }

    function normalizeResearchEvent(event) {
        return {
            stage: typeof event.stage === 'string' ? event.stage : '',
            iteration: Number(event.iteration || 0),
            max_iterations: Number(event.max_iterations || 0),
            status: typeof event.status === 'string' ? event.status : '',
            detail: typeof event.detail === 'string' ? event.detail : ''
        };
    }

    function researchStageSummary(task) {
        const stage = typeof task.research_controller_stage === 'string' ? task.research_controller_stage : '';
        if (!stage) return '대기 중';
        return `${researchStageLabel(stage)}${formatProgress(Number(task.research_controller_iteration || 0), Number(task.research_controller_max_iterations || 0))}`;
    }

    function qualityLoopSummary(task) {
        const status = typeof task.quality_status === 'string' ? task.quality_status : '';
        const current = Number(task.quality_current_iteration || 0);
        const max = Number(task.quality_max_iterations || 0);
        const parts = [];
        if (max > 0) parts.push(formatProgress(current, max).replace(/[()]/g, '').trim() || `${max}회`);
        if (status) parts.push(qualityStatusLabel(status));
        if (task.quality_depth) parts.push(String(task.quality_depth));
        return parts.join(' · ') || '검증 전';
    }

    function engineSummary(task) {
        const parts = [];
        if (task.engine_preset_name) parts.push(task.engine_preset_name);
        if (task.engine_kind) parts.push(task.engine_kind);
        if (task.resolved_model || task.model) parts.push(task.resolved_model || task.model);
        if (task.fallback_used === 'true') parts.push(`fallback${task.fallback_reason ? `: ${task.fallback_reason}` : ''}`);
        return parts.join(' · ') || '-';
    }

    function researchConfigSummary(task) {
        const parts = [];
        if (task.research_type) parts.push(labelResearchConfigValue(task.research_type));
        if (task.research_mode) parts.push(labelResearchConfigValue(task.research_mode));
        if (task.research_format) parts.push(labelResearchConfigValue(task.research_format));
        if (task.research_intensity) parts.push(`${task.research_intensity} intensity`);
        return parts.join(' · ') || '-';
    }

    function labelResearchConfigValue(value) {
        if (!value) return value;
        return researchValueLabels[value] || value;
    }

    function formatProgress(current, max) {
        if (max > 0 && current > 0) return ` (${current}/${max})`;
        if (max > 0) return ` (0/${max})`;
        return '';
    }

    function controllerStatusLabel(status) {
        const labels = {
            running: '진행',
            completed: '완료',
            failed: '결격'
        };
        return labels[status] || status || '기록';
    }

    function safeStatusClass(status) {
        return String(status).replace(/[^a-z0-9_-]/gi, '') || 'unknown';
    }

    function openRetryModal(task) {
        state.retryTaskTarget = task;
        elements.retryTargetText.textContent = `'${task.original_name}' 작업을 옵션을 바꿔 다시 실행합니다.`;
        renderRetryPresetOptions(task);
        elements.retryIntensitySelect.value = task.research_intensity || 'medium';
        elements.retryQualityIterationsSelect.value = String(task.quality_max_iterations || 2);
        elements.retryQualityDepthSelect.value = task.quality_depth || 'strict';
        elements.retryDeriveTaskCheckbox.checked = true;
        elements.retryModal.classList.remove('hidden');
    }

    function renderRetryPresetOptions(task) {
        const presets = (state.enginePresets || []).filter(preset => preset.enabled !== 'false');
        elements.retryEnginePresetSelect.innerHTML = '';
        presets.forEach(preset => {
            const option = document.createElement('option');
            option.value = preset.id;
            option.textContent = preset.name;
            elements.retryEnginePresetSelect.appendChild(option);
        });
        const currentPresetId = Number(task.engine_preset_id || 0);
        const fallbackPreset = presets.find(preset => preset.engine_kind === task.engine_kind) || presets[0];
        const selectedId = currentPresetId || (fallbackPreset ? fallbackPreset.id : null);
        if (selectedId) elements.retryEnginePresetSelect.value = String(selectedId);
    }

    async function startRetry() {
        const task = state.retryTaskTarget;
        if (!task) return;
        const enginePresetId = Number(elements.retryEnginePresetSelect.value);
        const payload = {
            derive_task: elements.retryDeriveTaskCheckbox.checked,
            engine_preset_id: enginePresetId || null,
            research_intensity: elements.retryIntensitySelect.value,
            research_quality_max_iterations: Number(elements.retryQualityIterationsSelect.value) || null,
            research_quality_depth: elements.retryQualityDepthSelect.value
        };
        const response = await api.retryTask(task.id, payload);
        if (response.ok) {
            elements.retryModal.classList.add('hidden');
            state.retryTaskTarget = null;
            actions.showToast('재시도 작업이 시작되었습니다.', '🔄');
            fetchTasks();
        } else {
            actions.showToast('재시도 실패', '❌');
        }
    }

    function closeRetryModal() {
        elements.retryModal.classList.add('hidden');
        state.retryTaskTarget = null;
    }

    function qualityStatusLabel(status) {
        const labels = {
            researching: '조사',
            repairing: '검색부채 보강',
            passed: '통과',
            failed: '실패',
            untrusted: '신뢰도 낮음',
            low_confidence: '신뢰도 낮음',
            no_confidence: '신뢰도 낮음'
        };
        return labels[status] || status;
    }

    function taskStatusLabel(task, fallback) {
        if (task.status === 'translating' && isScrapeTask(task)) return '스크랩 번역 중';
        if (isLowConfidenceResearch(task)) return '완료 · 신뢰도 낮음';
        if (task.status === 'completed') {
            const confidence = researchConfidenceLabel(task);
            return confidence ? `완료 · ${confidence}` : fallback;
        }
        if (task.status === 'failed') return isScrapeTask(task) ? '실패 · 스크랩' : '실패(기술적)';
        return fallback;
    }

    function researchConfidenceLabel(task) {
        const qualityStatus = typeof task.quality_status === 'string' ? task.quality_status : '';
        const qualityMax = Number(task.quality_max_iterations || 0);
        if (['untrusted', 'low_confidence', 'no_confidence'].includes(qualityStatus)) return '신뢰도 낮음';
        if (qualityStatus === 'passed') return '신뢰도 높음';
        if (task.status === 'completed' && qualityMax > 0) return '신뢰도 보통';
        return '';
    }

    function isLowConfidenceResearch(task) {
        const qualityStatus = typeof task.quality_status === 'string' ? task.quality_status : '';
        return ['untrusted', 'low_confidence', 'no_confidence'].includes(qualityStatus);
    }

    function isResearchTask(task) {
        return ['[Research]', '[AI-Research]'].includes(task.file_prefix);
    }

    function isScrapeTask(task) {
        return ['[Scrape]', '[Scrape+KO]'].includes(task.file_prefix) || isScrapeTranslateTask(task);
    }

    function isScrapeTranslateTask(task) {
        if (task.file_prefix !== '[KO]') return false;
        const cleanupFiles = parseJsonStringArray(task.cleanup_files);
        const sourceFilenames = parseJsonStringArray(task.source_filenames);
        return cleanupFiles.length > 0
            && sourceFilenames.length > 0
            && cleanupFiles.some(filename => sourceFilenames.includes(filename));
    }

    function parseJsonStringArray(raw) {
        if (typeof raw !== 'string' || raw.trim() === '') return [];
        try {
            const parsed = JSON.parse(raw);
            if (!Array.isArray(parsed)) return [];
            return parsed.filter(value => typeof value === 'string');
        } catch (_) {
            return [];
        }
    }

    function researchStageLabel(stage) {
        const labels = {
            plan: '계획',
            search: '검색',
            source_cards: '출처 카드',
            claim_log: '주장 로그',
            draft: '초안',
            quality_gate: '품질 게이트',
            repair: '검색부채',
            final: '최종화',
            untrusted: '신뢰도 없음'
        };
        return labels[stage] || stage;
    }

    return { setupTaskSSE, fetchTasks, fetchConfig, renderTaskList, startRetry, closeRetryModal };
}
