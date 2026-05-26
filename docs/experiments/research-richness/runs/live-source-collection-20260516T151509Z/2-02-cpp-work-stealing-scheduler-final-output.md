## 최종 답변 (Final Answer)

현대 C++에서 work-stealing scheduler를 구현할 때 널리 받아들여진 핵심 구조는 “각 worker가 자기 local deque를 소유하고, 일이 없어진 worker가 다른 worker의 deque에서 훔친다”는 분산형 스케줄링이다. oneTBB 문서와 Chase-Lev deque 논문 계열이 공통으로 보여주는 기본 모델은 owner thread가 deque의 bottom에서 push/pop을 수행하고, thief thread가 top에서 steal을 수행하는 방식이다. 이 구조는 fork-join, divide-and-conquer, DAG task runtime처럼 작업량이 동적으로 생기는 상황에서 중앙 큐 병목을 줄이고, owner의 최신 작업을 계속 실행해 cache locality를 얻는 데 유리하다.

구현의 중심은 worker pool, per-worker deque, global submission path, task wrapper, shutdown/cancellation 상태, exception 전달 정책이다. 일반적으로 외부 thread가 submit한 작업은 global queue나 round-robin으로 특정 worker deque에 넣고, worker 내부에서 spawn한 작업은 현재 worker의 local deque에 넣는다. worker loop는 “local pop → global/injection queue 확인 → victim 선택 후 steal → 대기 또는 backoff” 순서로 설계하는 것이 실용적이다. random victim 선택은 oneTBB와 Taskflow 문서/논문에서 확인되는 널리 쓰이는 baseline이지만, NUMA·cache hierarchy·latency가 큰 시스템에서는 locality-aware 또는 hierarchical stealing이 더 나을 수 있어 workload별 벤치마크 없이 “최선”이라고 단정하면 안 된다.

deque 구현은 가장 큰 위험 지점이다. mutex 기반 deque는 correctness baseline과 작은 규모 구현에는 적합하지만, 고성능 목표라면 Chase-Lev류 lock-free work-stealing deque가 대표적 선택지다. 다만 lock-free deque는 top/bottom index 경쟁, 마지막 원소를 두고 owner pop과 thief steal이 충돌하는 CAS 경로, circular buffer resize, empty 판정, index overflow/underflow, ABA와 reclamation 문제를 모두 다뤄야 한다. 특히 약한 메모리 모델에서 barrier와 `std::memory_order` 선택은 성능 최적화가 아니라 correctness 조건이다. 검증된 알고리즘을 그대로 이식하지 않고 “relaxed가 빠르다”는 이유로 임의 변경하면 ARM/POWER 같은 relaxed architecture에서만 드러나는 버그를 만들 수 있다.

task representation은 요구 API가 결정한다. `std::function<void()>`은 단순하지만 copyability, type erasure, heap allocation 가능성이 있고 move-only callable을 자연스럽게 담지 못한다. `std::packaged_task<R()>`는 `std::future`를 통한 결과·예외 전달이 필요할 때 표준적 선택이지만 shared state 비용과 one-shot 성격을 고려해야 한다. 고성능 scheduler는 보통 move-only small-buffer task wrapper, intrusive task node, coroutine handle, custom allocator를 검토한다. 여기서 “정답”은 없다. task가 수십 ns인지 수 ms인지, 결과 반환이 필요한지, cancellation이 필요한지, 재귀 spawn이 많은지에 따라 wrapper 비용이 전체 성능을 지배할 수도 있고 무시 가능할 수도 있다.

shutdown과 cancellation은 강제 종료가 아니라 협력적 프로토콜로 설계해야 한다. C++20 `std::stop_source`/`std::stop_token`은 stop request와 interruptible wait를 표현하는 데 유용하지만, 이미 실행 중인 임의 C++ callable을 안전하게 끊어내지는 않는다. 따라서 scheduler는 “더 이상 submit 받지 않기”, “대기 중 worker 깨우기”, “queue drain 또는 discard 정책”, “running task 완료 대기”, “join에서 예외/취소 상태 보고”를 명확히 분리해야 한다. exception은 worker thread 밖으로 새면 `std::terminate`로 이어질 수 있으므로 task boundary에서 잡아 `promise/future`, error sink, task group context, exception list 중 하나로 전달해야 한다. oneTBB처럼 예외가 관련 task group cancellation으로 이어지는 정책은 하나의 대표 사례이지, 모든 scheduler의 필수 규칙은 아니다.

벤치마킹은 단순 throughput 숫자 하나로 끝내면 안 된다. 최소한 단일 thread overhead, local push/pop 비용, steal 성공/실패 비용, submit latency, fork-join recursive workload, 불균형 DAG, 작은 task와 큰 task, worker 수 scaling, oversubscription, NUMA 배치, false sharing 민감도, shutdown latency를 나눠 측정해야 한다. Google Benchmark는 multithreaded benchmark와 wall-clock 측정을 지원하지만, scheduler 내부가 이미 thread를 만든다면 harness 설계가 결과를 왜곡할 수 있다. ThreadSanitizer는 data race 탐지에 유용하지만 큰 실행·메모리 overhead가 있으므로 correctness 검증용으로 쓰고 성능 수치와 섞지 않는 것이 안전하다.

확인된 사실과 불확실성을 분리하면 다음과 같다. 확인된 사실은 per-worker deque, owner-bottom/thief-top, random stealing baseline, Chase-Lev deque의 대표성, weak memory에서의 ordering 중요성, `packaged_task`/`future`의 exception 저장, C++20 stop token의 협력적 취소 성격이다. 불확실한 부분은 특정 modern C++ 구현에서 어떤 task wrapper가 가장 빠른지, random victim이 특정 NUMA 서버에서 locality-aware stealing보다 나은지, 특정 `memory_order` 조합이 모든 대상 architecture에서 안전하고 빠른지다. 실무 권고는 먼저 mutex deque 기반의 명확한 baseline과 테스트를 만든 뒤, 검증된 Chase-Lev 구현을 참조해 lock-free deque로 교체하고, correctness sanitizer/stress/litmus test와 실제 workload benchmark를 통과한 경우에만 최적화를 유지하는 것이다.## 구현 체크리스트

| 영역 | 널리 받아들여진 패턴 | 구현 선택지 | 반드시 검증할 것 |
|---|---|---|---|
| Architecture | worker별 local deque, idle worker의 steal | global injection queue, per-worker submit, priority/affinity queue | 중앙 큐 병목, starvation, oversubscription |
| Deque ownership | owner는 bottom, thief는 top | mutex deque, Chase-Lev, bounded ring, intrusive node | last-item race, empty 판정, resize, index overflow |
| Stealing protocol | idle worker가 victim 선택 후 steal | random, round-robin, NUMA-local-first, hierarchical | steal 실패율, remote cache/NUMA penalty, fairness |
| Memory ordering | atomic top/bottom과 acquire/release/CAS 설계 | 검증 논문 기반 C11/C++11 ordering, 보수적 seq_cst baseline | ARM/POWER/x86 stress, TSAN, randomized tests |
| Task representation | callable을 task boundary에서 실행 | `std::function`, `std::packaged_task`, move-only wrapper, coroutine | allocation 수, move/copy 비용, exception/result 전달 |
| Shutdown/cancel | cooperative stop, worker wakeup, join | drain, discard, cancel pending, task-group policy | submit-after-stop, wakeup loss, shutdown latency |
| Exception | task boundary에서 catch 후 전달 | future, exception_ptr list, fail-fast, group cancellation | worker terminate 방지, multiple exceptions policy |
| Benchmark | workload별 분리 측정 | Google Benchmark, custom harness, perf/VTune | tiny task distortion, false sharing, NUMA, sanitizer 분리 |## 대표 사례와 비교 관점

oneTBB는 work-stealing의 전형적 설명을 제공한다. worker는 자기 deque bottom에서 작업을 꺼내고, 일이 없으면 무작위로 선택한 다른 deque의 top에서 훔친다. Taskflow는 C++ task graph 실행기에서 work-stealing loop와 private queues를 사용하는 대표적인 modern C++ 사례다. Chase-Lev deque는 scheduler 내부 큐의 대표 연구 사례이며, weak memory model에서 correctness proof와 barrier 최적화 연구가 이어졌다. 이 사례들은 “구조적 패턴”을 뒷받침하지만, 그대로 복사할 API·성능 정책을 강제하지는 않는다.

비교하면, mutex deque는 구현·디버깅·테스트가 쉽고 baseline으로 적합하지만 contention이 커질 수 있다. lock-free Chase-Lev deque는 steal이 드문 정상 경로에서 빠를 수 있으나 검증 비용이 높다. random stealing은 단순하고 분산 contention을 줄이는 baseline이지만 NUMA locality를 보장하지 않는다. locality-aware stealing은 remote access를 줄일 가능성이 있으나 victim 선택 비용과 load balance 악화 가능성을 함께 측정해야 한다.

# 검증 부록

## 출처 감사 (Source Audit)
| ID | URL | Source | Class | Checked Fact | Limitation | Diagnostics |
| --- | --- | --- | --- | --- | --- | --- |
| S1 | https://supertech.mit.edu/biblio/scheduling-multithreaded-computations-work-stealing/ | Scheduling Multithreaded Computations by Work Stealing | academic/institutional | classic work-stealing research metadata | bibliographic page | - |
| S2 | https://www.cs.wm.edu/~dcschmidt/PDF/work-stealing-dequeue.pdf | Dynamic Circular Work-Stealing Deque | academic PDF | The common deque ownership model lets the owner push/pop at the bottom and thieves steal from the top. | PDF copy | - |
| S3 | https://researchportal.ip-paris.fr/fr/publications/correct-and-efficient-work-stealing-for-weak-memory-models-2/ | Correct and Efficient Work-Stealing for Weak Memory Models | academic/institutional | Chase-Lev deque is a representative central data structure for work-stealing schedulers. | abstract and metadata | - |
| S4 | https://uxlfoundation.github.io/oneTBB/main/tbb_userguide/How_Task_Scheduler_Works.html | How Task Scheduler Works | official documentation | A work-stealing scheduler commonly uses per-worker local deques and steal attempts by idle workers. | user guide not implementation source | - |
| S5 | https://taskflow.github.io/taskflow/icpads20.pdf | An Efficient Work-Stealing Scheduler for Task Dependency Graph | academic PDF | A work-stealing scheduler commonly uses per-worker local deques and steal attempts by idle workers. | specific DAG scheduler | - |
| S6 | https://taskflow.github.io/taskflow/2004.10908v2.pdf | Cpp-Taskflow v2 | academic PDF | The common deque ownership model lets the owner push/pop at the bottom and thieves steal from the top. | heterogeneous runtime concerns | - |
| S7 | https://en.cppreference.com/w/cpp/atomic/memory_order | std::memory_order | reference | Weak memory models make memory ordering and barrier placement a correctness and performance concern. | not ISO standard text | - |
| S8 | https://en.cppreference.com/cpp/thread/stop_source/request_stop | std::stop_source::request_stop | reference | C++ stop_source/stop_token supports cooperative cancellation but not arbitrary safe forced termination of running tasks. | not scheduler policy | - |
| S9 | https://en.cppreference.com/cpp/thread/packaged_task | std::packaged_task | reference | std::packaged_task is a valid task representation choice when future-based result and exception propagation are needed. | not performance evidence | - |
| S10 | https://clang.llvm.org/docs/ThreadSanitizer.html | ThreadSanitizer | official documentation | ThreadSanitizer is useful for race detection but its overhead means it should not be mixed with performance numbers. | tool overhead varies | - |
| S11 | https://github.com/google/benchmark/blob/main/docs/user_guide.md | Google Benchmark User Guide | official documentation | Google Benchmark can support threaded C++ benchmarking but does not replace scheduler-specific benchmark design. | not scheduler-specific methodology | - |
| S12 | https://en.cppreference.com/w/cpp/thread/hardware_destructive_interference_size | hardware_destructive_interference_size | reference | False sharing should be measured and mitigated in scheduler worker state and queue metadata. | implementation-defined | - |
| S13 | https://arxiv.org/pdf/1805.01768 | A New Analysis of Work Stealing with Latency | academic preprint | Latency and NUMA limit how far classic work-stealing results can be generalized. | preprint evidence | - |
| S14 | https://taskflow.github.io/taskflow/classtf_1_1Executor.html | tf::Executor Class Reference | official documentation | A work-stealing scheduler commonly uses per-worker local deques and steal attempts by idle workers. | API reference | - |
| S15 | https://uxlfoundation.github.io/oneTBB/main/tbb_userguide/Cancellation_and_Nested_Parallelism.html | Cancellation and Nested Parallelism | official documentation | Scheduler cancellation and exception handling are policy choices whose runtime behavior differs across libraries. | oneTBB-specific policy | - |
| S16 | https://oneapi-spec.uxlfoundation.org/specifications/oneapi/v1.2-rev-1/elements/onetbb/source/task_scheduler/scheduling_controls/task_group_context_cls | task_group_context | official specification | Scheduler cancellation and exception handling are policy choices whose runtime behavior differs across libraries. | oneTBB-specific specification | - |
## 주장 로그 (Claim Log)
| ID | Claim | Support | Confidence | Uncertainty |
| --- | --- | --- | --- | --- |
| C1 | A work-stealing scheduler commonly uses per-worker local deques and steal attempts by idle workers. | S4 (https://uxlfoundation.github.io/oneTBB/main/tbb_userguide/How_Task_Scheduler_Works.html); S5 (https://taskflow.github.io/taskflow/icpads20.pdf); S14 (https://taskflow.github.io/taskflow/classtf_1_1Executor.html) | high | - |
| C2 | The common deque ownership model lets the owner push/pop at the bottom and thieves steal from the top. | S2 (https://www.cs.wm.edu/~dcschmidt/PDF/work-stealing-dequeue.pdf); S4 (https://uxlfoundation.github.io/oneTBB/main/tbb_userguide/How_Task_Scheduler_Works.html); S6 (https://taskflow.github.io/taskflow/2004.10908v2.pdf) | high | - |
| C3 | Chase-Lev deque is a representative central data structure for work-stealing schedulers. | S2 (https://www.cs.wm.edu/~dcschmidt/PDF/work-stealing-dequeue.pdf); S3 (https://researchportal.ip-paris.fr/fr/publications/correct-and-efficient-work-stealing-for-weak-memory-models-2/) | high | - |
| C4 | Weak memory models make memory ordering and barrier placement a correctness and performance concern. | S3 (https://researchportal.ip-paris.fr/fr/publications/correct-and-efficient-work-stealing-for-weak-memory-models-2/); S7 (https://en.cppreference.com/w/cpp/atomic/memory_order) | high | - |
| C5 | std::packaged_task is a valid task representation choice when future-based result and exception propagation are needed. | S9 (https://en.cppreference.com/cpp/thread/packaged_task) | high | - |
| C6 | C++ stop_source/stop_token supports cooperative cancellation but not arbitrary safe forced termination of running tasks. | S8 (https://en.cppreference.com/cpp/thread/stop_source/request_stop) | medium_high | - |
| C7 | ThreadSanitizer is useful for race detection but its overhead means it should not be mixed with performance numbers. | S10 (https://clang.llvm.org/docs/ThreadSanitizer.html) | high | - |
| C8 | False sharing should be measured and mitigated in scheduler worker state and queue metadata. | S12 (https://en.cppreference.com/w/cpp/thread/hardware_destructive_interference_size) | medium_high | - |
| C9 | Latency and NUMA limit how far classic work-stealing results can be generalized. | S13 (https://arxiv.org/pdf/1805.01768) | medium_high | - |
| C10 | oneTBB and Taskflow are representative C++ task runtimes using work-stealing ideas. | S4 (https://uxlfoundation.github.io/oneTBB/main/tbb_userguide/How_Task_Scheduler_Works.html); S5 (https://taskflow.github.io/taskflow/icpads20.pdf); S6 (https://taskflow.github.io/taskflow/2004.10908v2.pdf); S14 (https://taskflow.github.io/taskflow/classtf_1_1Executor.html) | high | - |
| C11 | Google Benchmark can support threaded C++ benchmarking but does not replace scheduler-specific benchmark design. | S11 (https://github.com/google/benchmark/blob/main/docs/user_guide.md) | medium_high | - |
| C12 | Scheduler cancellation and exception handling are policy choices whose runtime behavior differs across libraries. | S15 (https://uxlfoundation.github.io/oneTBB/main/tbb_userguide/Cancellation_and_Nested_Parallelism.html); S16 (https://oneapi-spec.uxlfoundation.org/specifications/oneapi/v1.2-rev-1/elements/onetbb/source/task_scheduler/scheduling_controls/task_group_context_cls); S8 (https://en.cppreference.com/cpp/thread/stop_source/request_stop); S9 (https://en.cppreference.com/cpp/thread/packaged_task) | medium_high | - |
| C13 | Lock-free deque is a performance-oriented choice with higher verification cost, not an unconditional replacement for a mutex deque baseline. | S2 (https://www.cs.wm.edu/~dcschmidt/PDF/work-stealing-dequeue.pdf); S3 (https://researchportal.ip-paris.fr/fr/publications/correct-and-efficient-work-stealing-for-weak-memory-models-2/); S10 (https://clang.llvm.org/docs/ThreadSanitizer.html) | medium | - |
| C14 | Random victim selection is a widely used baseline, while NUMA/locality-aware stealing requires workload-specific benchmark validation. | S4 (https://uxlfoundation.github.io/oneTBB/main/tbb_userguide/How_Task_Scheduler_Works.html); S5 (https://taskflow.github.io/taskflow/icpads20.pdf); S13 (https://arxiv.org/pdf/1805.01768) | medium_high | - |
## 한계, 충돌, 연구 부채 (Limits, Conflicts, And Research Debt)
- Blocking unresolved conflicts: none recorded.
### Deferred / Caveated Conflicts
- K3: random victim selection vs locality/NUMA-aware selection [status=unresolved | deferred to research debt | Random stealing is a documented baseline, but evidence is insufficient to generalize it as best for NUMA or latency-sensitive workloads.]
- K4: task wrapper overhead generalization [status=unresolved | deferred to research debt | Relative performance of std::function, packaged_task, and custom move-only wrappers depends on task granularity and API requirements.]
### Research Debt
- D-K3 [open]: Comparative benchmark evidence for random victim selection versus locality-aware or NUMA-aware stealing in modern C++ schedulers. | next=collect benchmark papers or runtime docs with direct measurements; compare workload and hardware assumptions; avoid generalizing beyond tested hardware
- D-K4 [open]: Cross-runtime task representation overhead data for std::function, std::packaged_task, and custom move-only wrappers. | next=microbenchmark wrapper construction/invocation/allocation; separate tiny-task and coarse-task scenarios; record compiler and standard library versions
- D-MEM [open]: Additional validation of portable C++ memory_order choices for Chase-Lev implementations. | next=compare against published C11 pseudocode; run stress tests on weak-memory hardware or model checker; keep seq_cst baseline for differential testing
- debt-research-quality-gate-failed-authoritative-evidence-url-count [open]: Research quality gate failed: authoritative evidence URL count 2 is below required minimum 5 | next=Repair the failed quality gate item with stronger evidence or a narrower claim.
### Target Host / Source Class Misses
- query=`how implement work stealing scheduler modern core architecture official source` status=missed expected_host=unknown expected_source_class=official_or_primary provider=kakao reason=Earlier configured providers returned raw results but no topically relevant candidates: naver
- query=`how implement work stealing scheduler modern core architecture documentation` status=missed expected_host=unknown expected_source_class=official_or_primary provider=naver reason=Required source class official_or_primary was not recovered.
## 품질 게이트 (Quality Gate)
| Check | Result | Note |
| --- | --- | --- |
| deterministic gate status | failed | artifact-backed finalization rendered visible verification sections before validation |
| failure messages | review | Research quality gate failed: authoritative evidence URL count 2 is below required minimum 5 |
| unsupported claim count | 0 | lower is better |
| unresolved conflict count | 0 | unresolved items must stay visible as debt or limits |
| open debt count | 4 | open debt never counts as acceptance |
