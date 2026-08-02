document.addEventListener('DOMContentLoaded', () => {
    const appLoader = document.getElementById('app-loader');
    const loginOverlay = document.getElementById('login-overlay');
    const loginForm = document.getElementById('login-form');
    const loginCard = loginForm?.closest('.login-card');
    const passkeyInput = document.getElementById('passkey');
    const passkeyToggle = document.getElementById('passkey-toggle');
    const loginSubmit = document.getElementById('login-submit');
    const loginFeedback = document.getElementById('login-feedback');
    const appShell = document.getElementById('app-shell');
    const mainContent = document.getElementById('main-content');
    const sidebar = document.getElementById('sidebar');
    const sidebarToggle = document.getElementById('sidebar-toggle');
    const themeToggle = document.getElementById('theme-toggle');
    const breadcrumb = document.getElementById('breadcrumb');
    const navItems = document.querySelectorAll('.nav-item');
    const toastContainer = document.getElementById('toast-container');

    let overlay = null;
    let isUnlocking = false;

    // Reveal the login gate after loader
    setTimeout(() => {
        appLoader.classList.add('hidden');
        loginOverlay?.classList.remove('hidden');
        passkeyInput?.focus();
    }, 400);

    // Passkey visibility toggle
    passkeyToggle?.addEventListener('click', () => {
        const isPassword = passkeyInput.type === 'password';
        passkeyInput.type = isPassword ? 'text' : 'password';
        passkeyToggle.classList.toggle('showing', isPassword);
        passkeyToggle.setAttribute('aria-label', isPassword ? 'Hide passkey' : 'Show passkey');
    });

    // Inject x-security header on all admin requests
    document.body.addEventListener('htmx:configRequest', (evt) => {
        if (!evt.detail.path?.startsWith('/admin')) return;

        const passkey = passkeyInput?.value || sessionStorage.getItem('umbra-admin-passkey');
        if (passkey) {
            evt.detail.headers['x-security'] = passkey;
        }
    });

    // Handle login result
    document.body.addEventListener('htmx:afterRequest', (evt) => {
        if (evt.detail.requestConfig.path !== '/admin') return;

        setLoading(false);

        const status = evt.detail.xhr.status;

        if (status === 200) {
            const passkey = passkeyInput?.value;
            if (passkey) sessionStorage.setItem('umbra-admin-passkey', passkey);
            unlockDashboard();
        } else if (status === 401) {
            showLoginError('Invalid passkey. Please try again.');
        } else if (status === 404 && tryDemoUnlock()) {
            // static preview fallback: endpoint not found, demo passkey unlocks locally
        } else {
            showLoginError('Could not validate passkey. Please try again later.');
        }
    });

    document.body.addEventListener('htmx:sendError', (evt) => {
        if (evt.detail.requestConfig.path !== '/admin') return;

        setLoading(false);

        if (tryDemoUnlock()) {
            // static preview fallback: network error, demo passkey unlocks locally
        } else {
            showLoginError('Could not reach the admin endpoint. Is the server running?');
        }
    });

    function tryDemoUnlock() {
        if (passkeyInput?.value === 'umbra') {
            showLoginFeedback('Demo mode: no server detected, unlocking locally.', 'info');
            setTimeout(unlockDashboard, 400);
            return true;
        }
        return false;
    }

    // Auto-unlock if a passkey is already in sessionStorage for this browser session
    function autoUnlockFromSession() {
        const savedPasskey = sessionStorage.getItem('umbra-admin-passkey');
        if (!savedPasskey || !passkeyInput) return;

        passkeyInput.value = savedPasskey;
        setLoading(true);

        htmx.ajax('GET', '/admin', {
            swap: 'none',
            headers: { 'x-security': savedPasskey }
        });
    }

    setTimeout(autoUnlockFromSession, 600);

    function setLoading(loading) {
        loginSubmit?.classList.toggle('loading', loading);
        if (passkeyInput) passkeyInput.disabled = loading;
        if (loginSubmit) loginSubmit.disabled = loading;
    }

    function showLoginError(message) {
        if (!loginFeedback || !loginCard) return;
        loginFeedback.textContent = message;
        loginFeedback.classList.add('visible');
        loginCard.classList.remove('shake');
        void loginCard.offsetWidth; // reflow
        loginCard.classList.add('shake');
    }

    function showLoginFeedback(message) {
        if (!loginFeedback) return;
        loginFeedback.textContent = message;
        loginFeedback.classList.add('visible');
    }

    function clearLoginFeedback() {
        loginFeedback?.classList.remove('visible');
    }

    // Unlock animation: login dissolves, dashboard reveals
    function unlockDashboard() {
        if (isUnlocking) return;
        isUnlocking = true;

        clearLoginFeedback();
        loginOverlay?.classList.add('leaving');

        // Prepare shell behind the overlay so the reveal is immediate
        appShell.classList.remove('hidden');
        appShell.classList.add('revealing');

        setTimeout(() => {
            loginOverlay?.classList.add('hidden');
            loginOverlay?.classList.remove('leaving');

            // Load the dashboard fragment into the main content area
            htmx.ajax('GET', 'fragments/dashboard.html', {
                target: '#main-content',
                swap: 'innerHTML transition:true',
                indicator: '#content-loader'
            });

            showToast('Welcome to the Umbra SVM Indexer admin panel.', 'success', 4000);
        }, 500);
    }

    // Prevent default form submission fallback; HTMX handles it, but this keeps the UX tight
    loginForm?.addEventListener('submit', (e) => {
        e.preventDefault();
        clearLoginFeedback();
        setLoading(true);
    });

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
