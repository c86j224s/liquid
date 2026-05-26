export function openModal(element) {
    element.classList.remove('hidden');
}

export function closeModal(element) {
    element.classList.add('hidden');
}

export function bindBackdropClose(element, close) {
    element.addEventListener('click', (event) => {
        if (event.target === element) close();
    });
}
