document.addEventListener('DOMContentLoaded', () => {
    const appLoader = document.getElementById('app-loader');
    const appShell = document.getElementById('app-shell');
    const sidebar = document.getElementById('sidebar');
    const sidebarToggle = document.getElementById('sidebar-toggle');
    const themeToggle = document.getElementById('theme-toggle');
    const breadcrumb = document.getElementById('breadcrumb');
    const navItems = document.querySelectorAll('.nav-item');
    const toastContainer = document.getElementById('toast-container');

    let overlay = null;

    // Reveal the app with a smooth fade-in
    setTimeout(() => {
        appLoader.classList.add('hidden');
        appShell.style.opacity = '1';
    }, 400);

    // Create mobile overlay if needed
    function ensureOverlay() {
        if (overlay) return overlay;
        overlay = document.createElement('div');
        overlay.className = 'sidebar-overlay';
        overlay.addEventListener('click', closeSidebar);
        document.body.appendChild(overlay);
        return overlay;
    }

    function openSidebar() {
        sidebar.classList.add('open');
        ensureOverlay().classList.add('visible');
    }

    function closeSidebar() {
        sidebar.classList.remove('open');
        if (overlay) overlay.classList.remove('visible');
    }

    function toggleSidebar() {
        const isMobile = window.innerWidth <= 1024;
        if (isMobile) {
            sidebar.classList.contains('open') ? closeSidebar() : openSidebar();
        } else {
            appShell.classList.toggle('collapsed');
        }
    }

    sidebarToggle.addEventListener('click', toggleSidebar);

    // Theme toggle
    function applyTheme(theme) {
        document.documentElement.setAttribute('data-theme', theme);
        localStorage.setItem('umbra-theme', theme);
    }

    themeToggle.addEventListener('click', () => {
        const current = document.documentElement.getAttribute('data-theme');
        applyTheme(current === 'dark' ? 'light' : 'dark');
    });

    const savedTheme = localStorage.getItem('umbra-theme');
    if (savedTheme) applyTheme(savedTheme);

    // Active sidebar item + breadcrumb update
    function setActiveNav(target) {
        navItems.forEach(item => item.classList.remove('active'));
        target.classList.add('active');

        const label = target.querySelector('.nav-label')?.textContent || 'Panel';
        breadcrumb.innerHTML = `<span class="breadcrumb-current">${label}</span>`;

        if (window.innerWidth <= 1024) closeSidebar();
    }

    navItems.forEach(item => {
        item.addEventListener('click', (e) => {
            setActiveNav(item);
        });
    });

    // HTMX events: update active nav after a successful swap
    document.body.addEventListener('htmx:afterSwap', (evt) => {
        const trigger = evt.detail.requestConfig?.trigger;
        if (trigger && trigger.classList.contains('nav-item')) {
            setActiveNav(trigger);
        }
    });

    document.body.addEventListener('htmx:sendError', () => {
        showToast('Failed to load section. Make sure the panel is served by a local server.', 'error');
    });

    // Toast helper
    function showToast(message, type = 'info', duration = 4000) {
        const toast = document.createElement('div');
        toast.className = `toast toast-${type}`;

        const icon = type === 'success' ? '✓' : type === 'error' ? '!' : 'i';
        toast.innerHTML = `
            <span class="toast-icon">${icon}</span>
            <span class="toast-message">${message}</span>
        `;

        toastContainer.appendChild(toast);

        setTimeout(() => {
            toast.classList.add('leaving');
            toast.addEventListener('animationend', () => toast.remove());
        }, duration);
    }

    // Expose a tiny debug API on window for console tinkering
    window.umbra = {
        toast: showToast,
        theme: applyTheme,
        sidebar: { open: openSidebar, close: closeSidebar, toggle: toggleSidebar }
    };

    // Keyboard shortcuts
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && sidebar.classList.contains('open')) {
            closeSidebar();
        }
        if ((e.metaKey || e.ctrlKey) && e.key === 'b') {
            e.preventDefault();
            toggleSidebar();
        }
    });

    // Close mobile sidebar on resize to desktop
    window.addEventListener('resize', () => {
        if (window.innerWidth > 1024 && sidebar.classList.contains('open')) {
            closeSidebar();
        }
    });

    // Demo: show a welcome toast once on first load (only if local server is used)
    if (window.location.protocol !== 'file:') {
        setTimeout(() => {
            showToast('Admin panel shell loaded. Section bodies will be wired next.', 'info', 5000);
        }, 900);
    }
});
