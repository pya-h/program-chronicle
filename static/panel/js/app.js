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
    const sidebar = document.getElementById('sidebar');
    const sidebarToggle = document.getElementById('sidebar-toggle');
    const themeToggle = document.getElementById('theme-toggle');
    const breadcrumb = document.getElementById('breadcrumb');
    const navItems = document.querySelectorAll('.nav-item');
    const toastContainer = document.getElementById('toast-container');

    let overlay = null;
    let isUnlocking = false;

    // Persisted appearance preferences (theme / accent / animations), applied to
    // the document root and reflected into the Settings controls when present.
    const Prefs = (() => {
        const KEYS = { theme: 'umbra-theme', accent: 'umbra-accent', anim: 'umbra-anim' };
        const DEFAULTS = { theme: 'dark', accent: 'blue', anim: 'on' };
        const ACCENTS = {
            blue:    { rgb: '59, 130, 246',  c400: '#60a5fa', c500: '#3b82f6', c600: '#2563eb', c700: '#1d4ed8' },
            violet:  { rgb: '139, 92, 246',  c400: '#a78bfa', c500: '#8b5cf6', c600: '#7c3aed', c700: '#6d28d9' },
            emerald: { rgb: '16, 185, 129',  c400: '#34d399', c500: '#10b981', c600: '#059669', c700: '#047857' },
            rose:    { rgb: '244, 63, 94',   c400: '#fb7185', c500: '#f43f5e', c600: '#e11d48', c700: '#be123c' },
            amber:   { rgb: '245, 158, 11',  c400: '#fbbf24', c500: '#f59e0b', c600: '#d97706', c700: '#b45309' },
            cyan:    { rgb: '6, 182, 212',   c400: '#22d3ee', c500: '#06b6d4', c600: '#0891b2', c700: '#0e7490' }
        };
        const root = document.documentElement;
        const media = window.matchMedia('(prefers-color-scheme: dark)');
        const get = (k) => localStorage.getItem(KEYS[k]) || DEFAULTS[k];

        function applyTheme() {
            const pref = get('theme');
            root.setAttribute('data-theme', pref === 'system' ? (media.matches ? 'dark' : 'light') : pref);
        }

        function applyAccent() {
            const a = ACCENTS[get('accent')] || ACCENTS.blue;
            root.style.setProperty('--accent-rgb', a.rgb);
            root.style.setProperty('--accent-400', a.c400);
            root.style.setProperty('--accent-500', a.c500);
            root.style.setProperty('--accent-600', a.c600);
            root.style.setProperty('--accent-700', a.c700);
        }

        function applyAnim() {
            root.classList.toggle('no-anim', get('anim') === 'off');
        }

        function applyAll() { applyTheme(); applyAccent(); applyAnim(); }

        function reflect() {
            const panel = document.getElementById('settings-panel');
            if (!panel) return;
            panel.querySelectorAll('[data-set-theme]').forEach(b =>
                b.classList.toggle('active', b.dataset.setTheme === get('theme')));
            panel.querySelectorAll('[data-set-accent]').forEach(b =>
                b.classList.toggle('active', b.dataset.setAccent === get('accent')));
            const sw = panel.querySelector('[data-toggle-anim]');
            if (sw) {
                const on = get('anim') !== 'off';
                sw.classList.toggle('on', on);
                sw.setAttribute('aria-checked', String(on));
            }
        }

        function setTheme(v) { localStorage.setItem(KEYS.theme, v); applyTheme(); reflect(); }
        function setAccent(v) { localStorage.setItem(KEYS.accent, v); applyAccent(); reflect(); }
        function setAnim(on) { localStorage.setItem(KEYS.anim, on ? 'on' : 'off'); applyAnim(); reflect(); }
        function toggleTheme() { setTheme(root.getAttribute('data-theme') === 'dark' ? 'light' : 'dark'); }
        function reset() { Object.values(KEYS).forEach(k => localStorage.removeItem(k)); applyAll(); reflect(); }

        media.addEventListener('change', () => { if (get('theme') === 'system') applyTheme(); });

        return { setTheme, setAccent, setAnim, toggleTheme, reset, applyAll, reflect };
    })();

    Prefs.applyAll();

    document.body.addEventListener('click', (e) => {
        const themeBtn = e.target.closest('[data-set-theme]');
        if (themeBtn) return Prefs.setTheme(themeBtn.dataset.setTheme);

        const accentBtn = e.target.closest('[data-set-accent]');
        if (accentBtn) return Prefs.setAccent(accentBtn.dataset.setAccent);

        const animBtn = e.target.closest('[data-toggle-anim]');
        if (animBtn) return Prefs.setAnim(!animBtn.classList.contains('on'));

        const resetBtn = e.target.closest('[data-reset-prefs]');
        if (resetBtn) {
            Prefs.reset();
            showToast('Preferences reset to defaults.', 'info');
        }
    });

    setTimeout(() => {
        appLoader.classList.add('hidden');
        loginOverlay?.classList.remove('hidden');
        passkeyInput?.focus();
    }, 400);

    passkeyToggle?.addEventListener('click', () => {
        const isPassword = passkeyInput.type === 'password';
        passkeyInput.type = isPassword ? 'text' : 'password';
        passkeyToggle.classList.toggle('showing', isPassword);
        passkeyToggle.setAttribute('aria-label', isPassword ? 'Hide passkey' : 'Show passkey');
    });

    document.body.addEventListener('htmx:configRequest', (evt) => {
        if (!evt.detail.path?.startsWith('/admin')) return;
        const passkey = passkeyInput?.value || sessionStorage.getItem('umbra-admin-passkey');
        if (passkey) evt.detail.headers['x-security'] = passkey;
    });

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
            // Static preview: no Rust server, fall back to the demo passkey.
        } else {
            showLoginError('Could not validate passkey. Please try again later.');
        }
    });

    document.body.addEventListener('htmx:sendError', (evt) => {
        if (evt.detail.requestConfig.path !== '/admin') return;
        setLoading(false);
        if (!tryDemoUnlock()) {
            showLoginError('Could not reach the admin endpoint. Is the server running?');
        }
    });

    function tryDemoUnlock() {
        if (passkeyInput?.value === 'umbra') {
            showLoginFeedback('Demo mode: no server detected, unlocking locally.');
            setTimeout(unlockDashboard, 400);
            return true;
        }
        return false;
    }

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
        void loginCard.offsetWidth; // reflow so the shake animation can replay
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

    // Lock-screen handoff: reveal the shell behind the gate, then slide the gate
    // straight down off-screen to uncover the dashboard.
    function unlockDashboard() {
        if (isUnlocking) return;
        isUnlocking = true;
        clearLoginFeedback();

        appShell.classList.remove('hidden');
        appShell.classList.add('revealing');

        requestAnimationFrame(() => loginOverlay?.classList.add('leaving'));

        let done = false;
        const finish = () => {
            if (done) return;
            done = true;

            loginOverlay?.classList.add('hidden');
            loginOverlay?.classList.remove('leaving');
            appShell.classList.remove('revealing');

            htmx.ajax('GET', 'fragments/dashboard.html', {
                target: '#main-content',
                swap: 'innerHTML transition:true',
                indicator: '#content-loader'
            });
            showToast('Welcome to the Umbra SVM Indexer admin panel.', 'success', 4000);
        };

        loginOverlay?.addEventListener('animationend', function handler(e) {
            if (e.animationName !== 'login-slide-down') return;
            loginOverlay.removeEventListener('animationend', handler);
            finish();
        });
        setTimeout(finish, 820); // fallback if animationend never fires
    }

    loginForm?.addEventListener('submit', (e) => {
        e.preventDefault();
        clearLoginFeedback();
        setLoading(true);
    });

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
        overlay?.classList.remove('visible');
    }

    function toggleSidebar() {
        if (window.innerWidth <= 1024) {
            sidebar.classList.contains('open') ? closeSidebar() : openSidebar();
        } else {
            appShell.classList.toggle('collapsed');
        }
    }

    sidebarToggle.addEventListener('click', toggleSidebar);

    themeToggle.addEventListener('click', () => Prefs.toggleTheme());

    function setActiveNav(target) {
        navItems.forEach(item => item.classList.remove('active'));
        target.classList.add('active');

        const label = target.querySelector('.nav-label')?.textContent || 'Panel';
        breadcrumb.innerHTML = `<span class="breadcrumb-current">${label}</span>`;

        if (window.innerWidth <= 1024) closeSidebar();
    }

    navItems.forEach(item => {
        item.addEventListener('click', () => setActiveNav(item));
    });

    document.body.addEventListener('htmx:afterSwap', (evt) => {
        const trigger = evt.detail.requestConfig?.trigger;
        if (trigger?.classList.contains('nav-item')) setActiveNav(trigger);
        Prefs.reflect();
    });

    document.body.addEventListener('htmx:sendError', (evt) => {
        if (evt.detail.requestConfig?.path === '/admin') return;
        showToast('Failed to load section. Make sure the panel is served by a local server.', 'error');
    });

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

    window.umbra = {
        toast: showToast,
        prefs: Prefs,
        sidebar: { open: openSidebar, close: closeSidebar, toggle: toggleSidebar }
    };

    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && sidebar.classList.contains('open')) closeSidebar();
        if ((e.metaKey || e.ctrlKey) && e.key === 'b') {
            e.preventDefault();
            toggleSidebar();
        }
    });

    window.addEventListener('resize', () => {
        if (window.innerWidth > 1024 && sidebar.classList.contains('open')) closeSidebar();
    });

    if (window.location.protocol !== 'file:') {
        setTimeout(() => {
            showToast('Admin panel shell loaded. Section bodies will be wired next.', 'info', 5000);
        }, 900);
    }
});
