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
        if (evt.detail.headers['x-security']) return;
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
            initNotifications();
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

    // Section swap detection is handled near the end of the script so RPC and
    // Programs initializers share the same listener and do not double-fire.

    document.body.addEventListener('htmx:sendError', (evt) => {
        if (evt.detail.requestConfig?.path === '/admin') return;
        showToast('Failed to load section. Is the server running?', 'error');
    });

    function showToast(message, type = 'info', duration = 4000) {
        const toast = document.createElement('div');
        toast.className = `toast toast-${type}`;
        const icon = type === 'success' ? '✓' : type === 'error' ? '!' : 'i';
        toast.innerHTML = `<span class="toast-icon">${icon}</span><span class="toast-message">${escapeHtml(message)}</span>`;
        toastContainer.appendChild(toast);
        setTimeout(() => {
            toast.classList.add('leaving');
            toast.addEventListener('animationend', () => toast.remove());
        }, duration);
    }

    window.umbra = {
        toast: showToast,
        prefs: Prefs,
        sidebar: { open: openSidebar, close: closeSidebar, toggle: toggleSidebar },
        api: apiCall,
        buildQuery,
        createListSection,
        bytes: { toBase58, fromBase58, toBase64, base64ToBytes, bytesToHex, hexToBytes }
    };

    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') {
            const openModals = document.querySelectorAll('.modal-overlay.open');
            if (openModals.length) {
                openModals.forEach(m => { if (!m.hasAttribute('data-static')) closeModal(m.id); });
            } else if (sidebar.classList.contains('open')) {
                closeSidebar();
            }
        }
        if ((e.metaKey || e.ctrlKey) && e.key === 'b') {
            e.preventDefault();
            toggleSidebar();
        }
    });

    window.addEventListener('resize', () => {
        if (window.innerWidth > 1024 && sidebar.classList.contains('open')) closeSidebar();
    });

    if (window.location.protocol !== 'file:') {
        setTimeout(() => showToast('Welcome to the Umbra SVM Indexer admin panel.', 'info', 4000), 900);
    }

    // Notifications (indexer errors) --------------------------------------------
    const NOTIF_POLL_MS = 30000;
    let notifInited = false;

    // Bound once, after unlock: the bell polls unseen indexer errors and lets the
    // operator dismiss (mark seen) each one.
    function initNotifications() {
        if (notifInited) return;
        const toggle = document.getElementById('notif-toggle');
        const panel = document.getElementById('notif-panel');
        const list = document.getElementById('notif-list');
        if (!toggle || !panel || !list) return;
        notifInited = true;

        toggle.addEventListener('click', (e) => {
            e.stopPropagation();
            panel.hasAttribute('hidden') ? openNotifPanel() : closeNotifPanel();
        });
        document.addEventListener('click', (e) => {
            if (!panel.hasAttribute('hidden') && !e.target.closest('#notif-wrap')) closeNotifPanel();
        });
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && !panel.hasAttribute('hidden')) closeNotifPanel();
        });
        list.addEventListener('click', async (e) => {
            const btn = e.target.closest('[data-notif-dismiss]');
            if (!btn) return;
            setBtnLoading(btn, true);
            const res = await apiCall('PATCH', `/admin/errors/${btn.dataset.notifDismiss}`);
            if (!res.ok) { setBtnLoading(btn, false); return showToast(res.message, 'error'); }
            refreshNotifications();
        });

        refreshNotifications();
        setInterval(() => refreshNotifications(), NOTIF_POLL_MS);
    }

    function openNotifPanel() {
        document.getElementById('notif-panel')?.removeAttribute('hidden');
        document.getElementById('notif-toggle')?.setAttribute('aria-expanded', 'true');
        refreshNotifications();
    }

    function closeNotifPanel() {
        document.getElementById('notif-panel')?.setAttribute('hidden', '');
        document.getElementById('notif-toggle')?.setAttribute('aria-expanded', 'false');
    }

    // Silent on failure: this runs on a background poll, so a down server must not
    // spawn a toast every tick.
    async function refreshNotifications() {
        const res = await apiCall('GET', `/admin/errors${buildQuery({ seen: false, page: 1, per_page: 20 })}`);
        if (!res.ok) return;
        const rows = res.data?.data || [];
        const total = res.data?.total ?? rows.length;
        renderNotifBadge(total);
        renderNotifList(rows, total);
    }

    function renderNotifBadge(count) {
        const badge = document.getElementById('notif-badge');
        if (!badge) return;
        badge.textContent = count > 99 ? '99+' : String(count);
        badge.hidden = count === 0;
    }

    function renderNotifList(rows, total) {
        const list = document.getElementById('notif-list');
        const empty = document.getElementById('notif-empty');
        const count = document.getElementById('notif-count');
        if (count) count.textContent = total > 0 ? `${total} unread` : 'All clear';
        if (!list) return;
        if (rows.length === 0) {
            list.innerHTML = '';
            if (empty) empty.hidden = false;
            return;
        }
        if (empty) empty.hidden = true;
        list.innerHTML = rows.map(renderNotifRow).join('');
    }

    function renderNotifRow(err) {
        const meta = [];
        if (err.program_id != null) meta.push(`Program ${escapeHtml(err.program_id)}`);
        if (err.slot != null) meta.push(`Slot ${escapeHtml(formatCount(err.slot))}`);
        meta.push(escapeHtml(formatRelativeTime(err.created_at)));
        return `
            <div class="notif-item">
                <div class="notif-item-head">
                    <span class="notif-kind ${escapeHtml(err.kind)}">${escapeHtml(err.kind)}</span>
                    <button type="button" class="notif-dismiss" data-notif-dismiss="${escapeHtml(err.id)}" aria-label="Mark as read" title="Mark as read">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
                    </button>
                </div>
                <p class="notif-message">${escapeHtml(err.message)}</p>
                <div class="notif-meta">${meta.join(' · ')}</div>
            </div>
        `;
    }

    // Naive-UTC timestamps arrive without a zone; pin them to UTC before diffing.
    function formatRelativeTime(raw) {
        if (!raw) return '—';
        const iso = /[zZ]|[+-]\d{2}:?\d{2}$/.test(raw) ? raw : `${raw}Z`;
        const then = new Date(iso).getTime();
        if (!Number.isFinite(then)) return raw;
        const sec = Math.round((Date.now() - then) / 1000);
        if (sec < 60) return 'just now';
        const min = Math.round(sec / 60);
        if (min < 60) return `${min}m ago`;
        const hr = Math.round(min / 60);
        if (hr < 24) return `${hr}h ago`;
        const day = Math.round(hr / 24);
        if (day < 7) return `${day}d ago`;
        return new Date(then).toLocaleDateString();
    }

    // RPC section ---------------------------------------------------------------
    const BASE58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

    function toBase58(bytes) {
        if (!bytes || bytes.length === 0) return '';
        const base = 58n;
        let num = 0n;
        for (const b of bytes) num = (num << 8n) | BigInt(b);
        if (num === 0n) return BASE58[0];
        let out = '';
        while (num > 0n) {
            out = BASE58[Number(num % base)] + out;
            num /= base;
        }
        return out;
    }

    function escapeHtml(str) {
        return String(str).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
    }

    function getPasskey() {
        return sessionStorage.getItem('umbra-admin-passkey') || passkeyInput?.value || '';
    }

    async function apiCall(method, path, body = null) {
        const headers = { 'x-security': getPasskey() };
        if (body !== null) headers['Content-Type'] = 'application/json';
        try {
            const res = await fetch(path, { method, headers, body: body !== null ? JSON.stringify(body) : null });
            const text = await res.text();
            const data = text ? (() => { try { return JSON.parse(text); } catch { return { message: text }; } })() : null;
            if (!res.ok) return { ok: false, status: res.status, data, message: data?.message || `Request failed (${res.status})` };
            return { ok: true, status: res.status, data, message: '' };
        } catch (err) {
            return { ok: false, status: 0, data: null, message: err.message || 'Network error' };
        }
    }

    // Byte encoders for `data` payloads: the API already delivers `data` as
    // base64 and addresses/signatures as base58, so these cover the panel's
    // display needs (base64 → bytes → hex) plus a hex parser for discriminators.
    function toBase64(bytes) {
        let bin = '';
        for (const b of bytes) bin += String.fromCharCode(b);
        return btoa(bin);
    }

    function base64ToBytes(b64) {
        const bin = atob(b64);
        const out = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
        return out;
    }

    function bytesToHex(bytes) {
        return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
    }

    function hexToBytes(hex) {
        const clean = hex.trim().replace(/^0x/i, '');
        if (clean.length === 0 || clean.length % 2 !== 0 || /[^0-9a-f]/i.test(clean)) return null;
        const out = new Uint8Array(clean.length / 2);
        for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.substr(i * 2, 2), 16);
        return out;
    }

    // Serialize a params object into a query string, dropping null/undefined/blank
    // so unset slot bounds and filters never reach the endpoint.
    function buildQuery(params) {
        const pairs = Object.entries(params)
            .filter(([, v]) => v !== null && v !== undefined && v !== '')
            .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`);
        return pairs.length ? `?${pairs.join('&')}` : '';
    }

    // Reusable list-section scaffold shared by every paginated panel section
    // (pdas, transactions, events). It owns the page cursor, merges the
    // caller's filters/slot bounds with the page params, fetches the standard
    // { data, total, pages } envelope, and drives skeleton / empty / error / pager
    // states against the section's own elements.
    function createListSection(config) {
        const {
            rootId, emptyId, countId, pagerId,
            endpoint,                 // string | () => string|null (path, no query)
            perPage = 20,
            renderRows,               // (items) => html
            countText,                // (total) => string
            params = () => ({}),      // () => extra query params (filters + slot bounds)
        } = config;

        let page = 1;
        const el = (id) => (id ? document.getElementById(id) : null);
        const resolveEndpoint = () => (typeof endpoint === 'function' ? endpoint() : endpoint);

        function renderPager(pages) {
            const pager = el(pagerId);
            if (!pager) return;
            if (pages <= 1) { pager.innerHTML = ''; return; }
            pager.innerHTML = `
                <button type="button" class="pager-btn" data-pager="prev" ${page <= 1 ? 'disabled' : ''}>Prev</button>
                <span class="pager-status">Page ${page} of ${pages}</span>
                <button type="button" class="pager-btn" data-pager="next" ${page >= pages ? 'disabled' : ''}>Next</button>
            `;
        }

        async function load() {
            const root = el(rootId);
            if (!root) return;
            const empty = el(emptyId);
            const count = el(countId);

            const path = resolveEndpoint();
            if (!path) {
                // No target yet (e.g. no program picked): show the empty state but
                // keep the fragment's initial count hint untouched.
                root.innerHTML = '';
                if (empty) empty.hidden = false;
                renderPager(0);
                return;
            }

            root.innerHTML = `<div class="skeleton-table" aria-busy="true"><div class="skeleton-row"></div><div class="skeleton-row"></div><div class="skeleton-row"></div></div>`;
            if (empty) empty.hidden = true;
            if (count) count.textContent = 'Loading…';

            const query = buildQuery({ ...params(), page, per_page: perPage });
            const res = await apiCall('GET', `${path}${query}`);
            if (!res.ok) {
                root.innerHTML = '';
                if (empty) empty.hidden = false;
                if (count) count.textContent = 'Failed to load';
                showToast(res.message, 'error');
                renderPager(0);
                return;
            }

            const rows = res.data?.data || [];
            const total = res.data?.total ?? rows.length;
            const pages = res.data?.pages ?? 1;
            if (count) count.textContent = countText ? countText(total) : `${total} item${total === 1 ? '' : 's'}`;

            if (rows.length === 0) {
                root.innerHTML = '';
                if (empty) empty.hidden = false;
                renderPager(0);
                return;
            }
            if (empty) empty.hidden = true;
            root.innerHTML = renderRows(rows);
            renderPager(pages);
        }

        // A fresh element replaces the pager on every render, so binding once on
        // the container created for this instance never stacks listeners.
        el(pagerId)?.addEventListener('click', (e) => {
            const btn = e.target.closest('[data-pager]');
            if (!btn || btn.disabled) return;
            page = Math.max(1, page + (btn.dataset.pager === 'next' ? 1 : -1));
            load();
        });

        return {
            load,
            reload() { page = 1; return load(); },
            get page() { return page; }
        };
    }

    function normalizeHttpUrl(raw) {
        const url = raw.trim();
        if (!url) return null;
        return url.includes('://') ? url : `https://${url}`;
    }

    function normalizeWsUrl(raw) {
        const url = raw.trim();
        if (!url) return null;
        return url.includes('://') ? url : `wss://${url}`;
    }

    function validateHttpUrl(raw) {
        const url = normalizeHttpUrl(raw);
        if (!url) return { ok: false, message: 'HTTP URL is required.' };
        try {
            const parsed = new URL(url);
            if (!['http:', 'https:'].includes(parsed.protocol)) return { ok: false, message: 'HTTP URL must use http:// or https://.' };
            return { ok: true, value: url };
        } catch {
            return { ok: false, message: 'HTTP URL is not a valid URL.' };
        }
    }

    function validateWsUrl(raw) {
        const url = normalizeWsUrl(raw);
        if (!url) return { ok: true, value: null };
        try {
            const parsed = new URL(url);
            if (!['ws:', 'wss:'].includes(parsed.protocol)) return { ok: false, message: 'WebSocket URL must use ws:// or wss://.' };
            return { ok: true, value: url };
        } catch {
            return { ok: false, message: 'WebSocket URL is not a valid URL.' };
        }
    }

    function setBtnLoading(btn, loading) {
        btn?.classList.toggle('loading', loading);
        if (btn) btn.disabled = loading;
    }

    function showFieldFeedback(el, message) {
        if (!el) return;
        el.textContent = message;
        el.classList.toggle('visible', !!message);
    }

    function showBtnSuccess(btn) {
        if (!btn) return;
        btn.classList.add('success');
        setTimeout(() => btn.classList.remove('success'), 1500);
    }

    function activeModal() {
        return document.querySelector('.modal-overlay.open');
    }

    function openModal(id) {
        const modal = document.getElementById(id);
        if (!modal) return;
        modal.classList.add('open');
        modal.setAttribute('aria-hidden', 'false');
        document.body.style.overflow = 'hidden';
        const focusTarget = modal.querySelector('.input-field') || modal.querySelector('.modal-close');
        setTimeout(() => focusTarget?.focus(), 60);
    }

    function closeModal(id) {
        const modal = document.getElementById(id);
        if (!modal) return;
        modal.classList.remove('open');
        modal.setAttribute('aria-hidden', 'true');
        if (!activeModal()) document.body.style.overflow = '';
    }

    document.addEventListener('click', (e) => {
        const close = e.target.closest('[data-close-modal]');
        if (close) {
            const modal = close.closest('.modal-overlay');
            if (modal) closeModal(modal.id);
        }
        // Backdrop click closes a modal unless it is marked static (edit/delete),
        // which must only be dismissed via the X / Cancel button or on success.
        if (e.target.classList.contains('modal-overlay') && e.target.classList.contains('open') && !e.target.hasAttribute('data-static')) {
            closeModal(e.target.id);
        }
    });

    function initRpcSection() {
        loadRpcList();
        bindAddRpcForm();
        bindEditRpcForm();
        bindDeleteRpcConfirm();
    }

    async function loadRpcList() {
        const root = document.getElementById('rpc-list-root');
        const empty = document.getElementById('rpc-empty');
        const count = document.getElementById('rpc-count');
        if (!root) return;

        root.innerHTML = `<div class="skeleton-table" aria-busy="true"><div class="skeleton-row"></div><div class="skeleton-row"></div><div class="skeleton-row"></div></div>`;
        if (empty) empty.hidden = true;
        if (count) count.textContent = 'Loading…';

        const res = await apiCall('GET', '/admin/rpcs');
        if (!res.ok) {
            root.innerHTML = '';
            if (empty) empty.hidden = false;
            if (count) count.textContent = 'Failed to load';
            showToast(res.message, 'error');
            return;
        }

        const list = res.data?.data || [];
        window.__rpcCache = list;
        if (count) count.textContent = `${list.length} endpoint${list.length === 1 ? '' : 's'}`;
        if (list.length === 0) {
            root.innerHTML = '';
            if (empty) empty.hidden = false;
            return;
        }
        renderRpcList(list);
    }

    function renderRpcList(list) {
        const root = document.getElementById('rpc-list-root');
        if (!root) return;
        root.innerHTML = `<div class="rpc-list">${list.map(renderRpcRow).join('')}</div>`;
    }

    function renderRpcRow(item) {
        const rpc = item.rpc || item;
        const programs = item.programs || [];
        const hasPrograms = programs.length > 0;
        return `
            <div class="rpc-row" data-rpc-id="${escapeHtml(rpc.id)}">
                <div class="rpc-id">${escapeHtml(rpc.id)}</div>
                <div class="rpc-url-block">
                    <span class="rpc-url-label">HTTP</span>
                    <span class="rpc-url-value">${escapeHtml(rpc.http_url)}</span>
                </div>
                <div class="rpc-url-block">
                    <span class="rpc-url-label">WebSocket</span>
                    <span class="rpc-url-value">${escapeHtml(rpc.ws_url)}</span>
                </div>
                <div class="rpc-actions">
                    ${hasPrograms ? `
                    <button type="button" class="rpc-action-btn accent" data-rpc-id="${escapeHtml(rpc.id)}" data-action="programs" aria-label="Programs">
                        <span class="rpc-programs-badge">
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
                            <span class="badge-count">${programs.length}</span>
                        </span>
                    </button>` : ''}
                    <button type="button" class="rpc-action-btn accent" data-rpc-id="${escapeHtml(rpc.id)}" data-action="edit" aria-label="Edit">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
                    </button>
                    <button type="button" class="rpc-action-btn danger" data-rpc-id="${escapeHtml(rpc.id)}" data-action="delete" aria-label="Delete">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
                    </button>
                </div>
            </div>
        `;
    }

    function bindAddRpcForm() {
        const form = document.getElementById('rpc-add-form');
        if (!form) return;
        form.addEventListener('submit', async (e) => {
            e.preventDefault();
            const feedback = document.getElementById('rpc-add-feedback');
            const btn = document.getElementById('rpc-add-submit');
            const httpInput = document.getElementById('rpc-add-http');
            const wsInput = document.getElementById('rpc-add-ws');
            showFieldFeedback(feedback, '');

            const http = validateHttpUrl(httpInput.value);
            if (!http.ok) return showFieldFeedback(feedback, http.message);
            const ws = validateWsUrl(wsInput.value);
            if (!ws.ok) return showFieldFeedback(feedback, ws.message);

            const payload = { http_url: http.value };
            if (ws.value) payload.ws_url = ws.value;

            setBtnLoading(btn, true);
            const res = await apiCall('POST', '/admin/rpcs', payload);
            setBtnLoading(btn, false);

            if (!res.ok) return showFieldFeedback(feedback, res.message);
            form.reset();
            showBtnSuccess(btn);
            showToast('RPC added successfully.', 'success');
            loadRpcList();
        });
    }

    let editOriginal = { http: '', ws: '' };

    document.body.addEventListener('click', (e) => {
        const btn = e.target.closest('[data-action="edit"]');
        if (!btn || !btn.dataset.rpcId) return;
        const id = btn.dataset.rpcId;
        const row = document.querySelector(`.rpc-row[data-rpc-id="${id}"]`);
        if (!row) return;
        const values = row.querySelectorAll('.rpc-url-value');
        editOriginal = { http: values[0]?.textContent || '', ws: values[1]?.textContent || '' };
        document.getElementById('rpc-edit-id').value = id;
        document.getElementById('rpc-edit-http').value = editOriginal.http;
        document.getElementById('rpc-edit-ws').value = editOriginal.ws;
        showFieldFeedback(document.getElementById('rpc-edit-feedback'), '');
        openModal('rpc-edit-modal');
    });

    function bindEditRpcForm() {
        const form = document.getElementById('rpc-edit-form');
        if (!form) return;
        form.addEventListener('submit', async (e) => {
            e.preventDefault();
            const feedback = document.getElementById('rpc-edit-feedback');
            const btn = document.getElementById('rpc-edit-submit');
            const id = document.getElementById('rpc-edit-id').value;
            const httpRaw = document.getElementById('rpc-edit-http').value;
            const wsRaw = document.getElementById('rpc-edit-ws').value;
            showFieldFeedback(feedback, '');

            // Only send fields that are non-empty AND differ from the loaded values,
            // so an unchanged form is rejected client-side (spec: at least one change).
            const payload = {};
            if (httpRaw.trim()) {
                const http = validateHttpUrl(httpRaw);
                if (!http.ok) return showFieldFeedback(feedback, http.message);
                if (http.value !== editOriginal.http) payload.http_url = http.value;
            }
            if (wsRaw.trim()) {
                const ws = validateWsUrl(wsRaw);
                if (!ws.ok) return showFieldFeedback(feedback, ws.message);
                if (ws.value !== editOriginal.ws) payload.ws_url = ws.value;
            }
            if (Object.keys(payload).length === 0) return showFieldFeedback(feedback, 'Change at least one field to save.');

            setBtnLoading(btn, true);
            const res = await apiCall('PATCH', `/admin/rpcs/${id}`, payload);
            setBtnLoading(btn, false);

            if (!res.ok) return showFieldFeedback(feedback, res.message);
            closeModal('rpc-edit-modal');
            showToast('RPC updated successfully.', 'success');
            loadRpcList();
        });
    }

    let deleteTargetId = null;

    document.body.addEventListener('click', (e) => {
        const btn = e.target.closest('[data-action="delete"]');
        if (!btn || !btn.dataset.rpcId) return;
        deleteTargetId = btn.dataset.rpcId;
        const row = document.querySelector(`.rpc-row[data-rpc-id="${deleteTargetId}"]`);
        const detail = document.getElementById('rpc-delete-detail');
        if (detail) detail.textContent = row?.querySelectorAll('.rpc-url-value')[0]?.textContent || '';
        showFieldFeedback(document.getElementById('rpc-delete-feedback'), '');
        openModal('rpc-delete-modal');
    });

    function bindDeleteRpcConfirm() {
        const btn = document.getElementById('rpc-delete-confirm');
        if (!btn) return;
        btn.addEventListener('click', async () => {
            if (!deleteTargetId) return;
            const feedback = document.getElementById('rpc-delete-feedback');
            showFieldFeedback(feedback, '');
            setBtnLoading(btn, true);
            const res = await apiCall('DELETE', `/admin/rpcs/${deleteTargetId}`);
            setBtnLoading(btn, false);
            if (!res.ok) return showFieldFeedback(feedback, res.message);
            deleteTargetId = null;
            closeModal('rpc-delete-modal');
            showToast('RPC deleted.', 'success');
            loadRpcList();
        });
    }

    document.body.addEventListener('click', (e) => {
        const btn = e.target.closest('[data-action="programs"]');
        if (!btn) return;
        const id = btn.dataset.rpcId;
        const item = (window.__rpcCache || []).find(i => String(i.rpc?.id || i.id) === id);
        renderProgramsModal(item?.programs || []);
        openModal('rpc-programs-modal');
    });

    function renderProgramsModal(programs, opts = {}) {
        const list = document.getElementById('rpc-programs-list');
        const title = document.getElementById('rpc-programs-title');
        if (title) title.textContent = `Programs using this RPC (${programs.length})`;
        if (!list) return;
        if (programs.length === 0) {
            list.innerHTML = '<p class="modal-text-muted">No programs tied to this RPC.</p>';
            return;
        }
        const clickable = opts.clickable !== false;
        list.innerHTML = programs.map(p => {
            const address = formatProgramAddress(p.address);
            return `
                <div class="program-item ${clickable ? 'program-item-clickable' : ''}" ${clickable ? `data-program-id="${escapeHtml(p.id)}" data-program-address="${escapeHtml(address)}"` : ''}>
                    <span class="program-name">${escapeHtml(p.name || `Program ${p.id}`)}</span>
                    <span class="program-address">${escapeHtml(address)}</span>
                    <span class="program-meta">ID: ${escapeHtml(p.id)}${p.last_processed_slot ? ` • Slot ${escapeHtml(p.last_processed_slot)}` : ''}</span>
                </div>
            `;
        }).join('');

        if (clickable) {
            list.querySelectorAll('.program-item-clickable').forEach(item => {
                item.addEventListener('click', () => {
                    const id = item.dataset.programId;
                    const address = item.dataset.programAddress;
                    closeModal('rpc-programs-modal');
                    openProgramEditById(id, address);
                });
            });
        }
    }

    // Programs section --------------------------------------------------------
    function formatProgramAddress(raw) {
        if (typeof raw === 'string') return raw;
        if (Array.isArray(raw)) return toBase58(raw.map(n => Number(n)));
        return '';
    }

    const BASE58_CHARS = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

    function fromBase58(str) {
        const base = 58n;
        let num = 0n;
        for (const ch of str) {
            const idx = BASE58_CHARS.indexOf(ch);
            if (idx === -1) return null;
            num = (num * base) + BigInt(idx);
        }
        const bytes = [];
        while (num > 0n) {
            bytes.unshift(Number(num & 0xffn));
            num >>= 8n;
        }
        for (let i = 0; i < str.length && str[i] === BASE58_CHARS[0]; i++) bytes.unshift(0);
        return bytes;
    }

    function validateSolanaAddress(raw) {
        const value = raw.trim();
        if (!value) return { ok: false, message: 'Program address is required.' };
        if (value.length < 32 || value.length > 44) return { ok: false, message: 'Address length does not look like a Solana public key.' };
        const bytes = fromBase58(value);
        if (!bytes) return { ok: false, message: 'Address contains invalid base-58 characters.' };
        if (bytes.length !== 32) return { ok: false, message: `Address decodes to ${bytes.length} bytes; expected 32.` };
        return { ok: true, value };
    }

    function initProgramsSection() {
        loadProgramList();
        bindAddProgramForm();
        bindEditProgramForm();
        bindDeleteProgramConfirm();
        bindProgramRpcModeSwitch('program-add');
        bindProgramRpcModeSwitch('program-edit');
    }

    async function loadProgramList() {
        const root = document.getElementById('program-list-root');
        const empty = document.getElementById('program-empty');
        const count = document.getElementById('program-count');
        if (!root) return;

        root.innerHTML = `<div class="skeleton-table" aria-busy="true"><div class="skeleton-row"></div><div class="skeleton-row"></div><div class="skeleton-row"></div></div>`;
        if (empty) empty.hidden = true;
        if (count) count.textContent = 'Loading…';

        const res = await apiCall('GET', '/admin/programs');
        if (!res.ok) {
            root.innerHTML = '';
            if (empty) empty.hidden = false;
            if (count) count.textContent = 'Failed to load';
            showToast(res.message, 'error');
            return;
        }

        const list = res.data?.data || [];
        window.__programCache = list;
        if (count) count.textContent = `${list.length} program${list.length === 1 ? '' : 's'}`;
        if (list.length === 0) {
            root.innerHTML = '';
            if (empty) empty.hidden = false;
            return;
        }
        renderProgramList(list);
    }

    function renderProgramList(list) {
        const root = document.getElementById('program-list-root');
        if (!root) return;
        root.innerHTML = `<div class="program-list">${list.map(renderProgramRow).join('')}</div>`;
    }

    function renderProgramRow(item) {
        const program = item.program || item;
        const rpc = item.rpc || {};
        const address = formatProgramAddress(program.address);
        const name = program.name || `Program ${program.id}`;
        const slot = program.last_processed_slot;
        const active = program.active !== false;
        const toggleLabel = active ? 'Deactivate' : 'Activate';
        const toggleIcon = active
            ? '<rect x="6" y="5" width="4" height="14"/><rect x="14" y="5" width="4" height="14"/>'
            : '<polygon points="6 4 20 12 6 20 6 4"/>';
        return `
            <div class="program-row" data-program-id="${escapeHtml(program.id)}" data-program-address="${escapeHtml(address)}">
                <div class="program-id">${escapeHtml(program.id)}</div>
                <div class="program-info">
                    <span class="program-name">${escapeHtml(name)}</span>
                    <span class="program-address" title="${escapeHtml(address)}">${escapeHtml(address)}</span>
                </div>
                <div class="program-rpc-link">
                    <button type="button" class="program-rpc-btn" data-program-id="${escapeHtml(program.id)}" data-action="rpc">
                        <span class="rpc-url-label">RPC</span>
                        <span class="program-rpc-url">${escapeHtml(rpc.http_url || '—')}</span>
                    </button>
                </div>
                <div class="program-status">
                    <span class="status-badge ${active ? (slot ? 'indexed' : 'pending') : 'paused'}">${active ? (slot ? `Slot ${escapeHtml(slot)}` : 'Pending') : 'Paused'}</span>
                </div>
                <div class="program-actions">
                    <button type="button" class="program-action-btn ${active ? 'warn' : 'success'}" data-program-id="${escapeHtml(program.id)}" data-active="${active}" data-action="toggle-active" aria-label="${toggleLabel}" title="${toggleLabel}">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${toggleIcon}</svg>
                    </button>
                    <button type="button" class="program-action-btn accent" data-program-id="${escapeHtml(program.id)}" data-action="edit" aria-label="Edit">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
                    </button>
                    <button type="button" class="program-action-btn danger" data-program-id="${escapeHtml(program.id)}" data-action="delete" aria-label="Delete">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
                    </button>
                </div>
            </div>
        `;
    }

    async function populateRpcSelect(selectId, includeKeepCurrent = false) {
        const select = document.getElementById(selectId);
        if (!select) return;

        const currentValue = select.value;
        select.innerHTML = '';
        const placeholder = document.createElement('option');
        placeholder.value = '';
        placeholder.textContent = includeKeepCurrent ? 'Keep current RPC' : 'Select an RPC endpoint…';
        placeholder.disabled = true;
        placeholder.selected = true;
        select.appendChild(placeholder);

        const res = await apiCall('GET', '/admin/rpcs');
        const rpcs = res.ok ? (res.data?.data || []) : [];
        if (res.ok) window.__rpcCache = rpcs;

        rpcs.forEach(item => {
            const rpc = item.rpc || item;
            const option = document.createElement('option');
            option.value = rpc.id;
            option.textContent = `${rpc.id}: ${rpc.http_url}`;
            select.appendChild(option);
        });

        if (currentValue) select.value = currentValue;
    }

    function bindProgramRpcModeSwitch(prefix) {
        const existing = document.getElementById(`${prefix}-mode-existing`);
        const newBtn = document.getElementById(`${prefix}-mode-new`);
        const panels = document.getElementById(`${prefix}-rpc-panels`);
        if (!existing || !newBtn || !panels) return;

        const setMode = (mode) => {
            existing.classList.toggle('active', mode === 'existing');
            newBtn.classList.toggle('active', mode === 'new');
            panels.querySelectorAll('.program-rpc-panel').forEach(p => p.classList.toggle('active', p.dataset.panel === mode));
        };

        existing.addEventListener('click', () => setMode('existing'));
        newBtn.addEventListener('click', () => setMode('new'));
    }

    // optional=true (update flow): an empty "New RPC" panel means "keep current RPC"
    // rather than a validation error, since the RPC is not required on patch.
    function getProgramRpcPayload(prefix, { optional = false } = {}) {
        const existingPanel = document.getElementById(`${prefix}-existing-panel`);
        const isExisting = existingPanel?.classList.contains('active') ?? true;
        if (isExisting) {
            const rpcId = document.getElementById(`${prefix}-rpc-id`)?.value;
            return rpcId ? { rpc_id: parseInt(rpcId, 10) } : {};
        }
        const httpRaw = (document.getElementById(`${prefix}-rpc-http`)?.value || '').trim();
        const wsRaw = (document.getElementById(`${prefix}-rpc-ws`)?.value || '').trim();
        if (optional && !httpRaw && !wsRaw) return {};
        const http = validateHttpUrl(httpRaw);
        if (!http.ok) return { error: http.message };
        const ws = validateWsUrl(wsRaw);
        if (!ws.ok) return { error: ws.message };
        const payload = { rpc_url: { http_url: http.value } };
        if (ws.value) payload.rpc_url.ws_url = ws.value;
        return payload;
    }

    // Reads an account-filters textarea. Empty → { value: undefined } (omit the field); valid JSON →
    // { value: <object> }; malformed → { error }.
    function parseAccountFilters(fieldId) {
        const raw = (document.getElementById(fieldId)?.value || '').trim();
        if (!raw) return { value: undefined };
        try {
            return { value: JSON.parse(raw) };
        } catch {
            return { error: 'Account filters must be valid JSON.' };
        }
    }

    function bindAddProgramForm() {
        const form = document.getElementById('program-add-form');
        if (!form) return;

        populateRpcSelect('program-add-rpc-id');

        form.addEventListener('submit', async (e) => {
            e.preventDefault();
            const feedback = document.getElementById('program-add-feedback');
            const btn = document.getElementById('program-add-submit');
            showFieldFeedback(feedback, '');

            const address = validateSolanaAddress(document.getElementById('program-add-address').value);
            if (!address.ok) return showFieldFeedback(feedback, address.message);

            const startSlotRaw = document.getElementById('program-add-start-slot').value;
            const startSlot = startSlotRaw ? parseInt(startSlotRaw, 10) : null;
            if (startSlotRaw && (!Number.isFinite(startSlot) || startSlot < 0)) {
                return showFieldFeedback(feedback, 'Start slot must be a non-negative integer.');
            }

            const rpcPayload = getProgramRpcPayload('program-add');
            if (rpcPayload.error) return showFieldFeedback(feedback, rpcPayload.error);
            if (!rpcPayload.rpc_id && !rpcPayload.rpc_url) {
                return showFieldFeedback(feedback, 'Select an existing RPC or provide a new RPC URL.');
            }

            const filters = parseAccountFilters('program-add-account-filters');
            if (filters.error) return showFieldFeedback(feedback, filters.error);

            const name = document.getElementById('program-add-name').value.trim() || null;
            const payload = { address: address.value, ...rpcPayload };
            if (name) payload.name = name;
            if (startSlot !== null) payload.start_slot = startSlot;
            if (filters.value !== undefined) payload.account_filters = filters.value;

            setBtnLoading(btn, true);
            const res = await apiCall('POST', '/admin/programs', payload);
            setBtnLoading(btn, false);

            if (!res.ok) return showFieldFeedback(feedback, res.message);
            form.reset();
            resetProgramRpcMode('program-add');
            showBtnSuccess(btn);
            showToast('Program added successfully.', 'success');
            loadProgramList();
        });
    }

    function resetProgramRpcMode(prefix) {
        const existing = document.getElementById(`${prefix}-mode-existing`);
        if (existing) {
            existing.click();
            const select = document.getElementById(`${prefix}-rpc-id`);
            if (select) select.value = '';
        }
    }

    let programEditOriginal = {};
    let programEditTargetId = null;

    document.body.addEventListener('click', (e) => {
        const btn = e.target.closest('[data-action="edit"]');
        if (!btn || !btn.dataset.programId) return;
        const id = btn.dataset.programId;
        const address = btn.closest('.program-row')?.dataset?.programAddress;
        openProgramEditById(id, address);
    });

    async function openProgramEditById(id, knownAddress) {
        programEditTargetId = id;
        const cached = window.__programCache || [];
        let item = cached.find(i => String(i.program?.id || i.id) === id);
        if (!item && knownAddress) {
            item = cached.find(i => formatProgramAddress(i.program?.address || i.address) === knownAddress);
        }
        if (!item) {
            const res = await apiCall('GET', `/admin/programs/${id}`);
            if (!res.ok) {
                showToast(res.message, 'error');
                return;
            }
            item = res.data;
        }
        if (!item) return;

        const program = item.program || item;
        const address = knownAddress || formatProgramAddress(program.address);

        programEditOriginal = {
            address,
            name: program.name || '',
            rpc_id: String(program.rpc_id || ''),
            // compact form for change detection; the textarea shows a pretty-printed copy
            account_filters: program.account_filters ? JSON.stringify(program.account_filters) : ''
        };

        const switchToPrograms = () => {
            const main = document.getElementById('main-content');
            if (main?.querySelector('[data-section="programs"]')) {
                setActiveNav(document.querySelector('.nav-item[hx-get="fragments/programs.html"]'));
                return Promise.resolve();
            }
            const navItem = document.querySelector('.nav-item[hx-get="fragments/programs.html"]');
            if (!navItem) return Promise.resolve();
            setActiveNav(navItem);
            return new Promise((resolve) => {
                const handler = () => {
                    document.body.removeEventListener('htmx:afterSwap', handler);
                    resolve();
                };
                document.body.addEventListener('htmx:afterSwap', handler);
                navItem.click();
            });
        };

        await switchToPrograms();

        const idEl = document.getElementById('program-edit-id');
        if (!idEl) { showToast('Could not open the program editor.', 'error'); return; }

        await populateRpcSelect('program-edit-rpc-id', true);

        idEl.value = program.id;
        document.getElementById('program-edit-address').value = programEditOriginal.address;
        document.getElementById('program-edit-name').value = programEditOriginal.name;
        const filtersField = document.getElementById('program-edit-account-filters');
        if (filtersField) {
            filtersField.value = program.account_filters
                ? JSON.stringify(program.account_filters, null, 2)
                : '';
        }
        const rpcSelect = document.getElementById('program-edit-rpc-id');
        if (rpcSelect) rpcSelect.value = '';

        resetProgramRpcMode('program-edit');
        showFieldFeedback(document.getElementById('program-edit-feedback'), '');
        openModal('program-edit-modal');
    }

    function bindEditProgramForm() {
        const form = document.getElementById('program-edit-form');
        if (!form) return;

        form.addEventListener('submit', async (e) => {
            e.preventDefault();
            const feedback = document.getElementById('program-edit-feedback');
            const btn = document.getElementById('program-edit-submit');
            showFieldFeedback(feedback, '');

            const addressRaw = document.getElementById('program-edit-address').value;
            const nameRaw = document.getElementById('program-edit-name').value;
            const rpcPayload = getProgramRpcPayload('program-edit', { optional: true });
            if (rpcPayload.error) return showFieldFeedback(feedback, rpcPayload.error);

            const payload = {};

            if (addressRaw.trim()) {
                const address = validateSolanaAddress(addressRaw);
                if (!address.ok) return showFieldFeedback(feedback, address.message);
                if (address.value !== programEditOriginal.address) payload.address = address.value;
            }

            if (nameRaw.trim()) {
                const name = nameRaw.trim();
                if (name !== programEditOriginal.name) payload.name = name;
            } else if (programEditOriginal.name) {
                payload.name = '';
            }

            if (rpcPayload.rpc_id) {
                if (String(rpcPayload.rpc_id) !== programEditOriginal.rpc_id) payload.rpc_id = rpcPayload.rpc_id;
            } else if (rpcPayload.rpc_url) {
                payload.rpc_url = rpcPayload.rpc_url;
            }

            const filters = parseAccountFilters('program-edit-account-filters');
            if (filters.error) return showFieldFeedback(feedback, filters.error);
            if (filters.value !== undefined) {
                // send only when the descriptor actually changed
                if (JSON.stringify(filters.value) !== programEditOriginal.account_filters) {
                    payload.account_filters = filters.value;
                }
            } else if (programEditOriginal.account_filters) {
                // cleared textarea → empty descriptor resets to track-all
                payload.account_filters = {};
            }

            if (Object.keys(payload).length === 0) {
                return showFieldFeedback(feedback, 'Change at least one field to save.');
            }

            setBtnLoading(btn, true);
            const res = await apiCall('PATCH', `/admin/programs/${programEditTargetId}`, payload);
            setBtnLoading(btn, false);

            if (!res.ok) return showFieldFeedback(feedback, res.message);
            closeModal('program-edit-modal');
            showToast('Program updated successfully.', 'success');
            loadProgramList();
        });
    }

    let deleteProgramTargetId = null;

    document.body.addEventListener('click', (e) => {
        const btn = e.target.closest('[data-action="delete"]');
        if (!btn || !btn.dataset.programId) return;
        deleteProgramTargetId = btn.dataset.programId;
        const row = document.querySelector(`.program-row[data-program-id="${deleteProgramTargetId}"]`);
        const detail = document.getElementById('program-delete-detail');
        if (detail) {
            const name = row?.querySelector('.program-name')?.textContent || '';
            const address = row?.dataset?.programAddress || '';
            detail.textContent = `${name}${address ? ` • ${address}` : ''}`;
        }
        showFieldFeedback(document.getElementById('program-delete-feedback'), '');
        openModal('program-delete-modal');
    });

    function bindDeleteProgramConfirm() {
        const btn = document.getElementById('program-delete-confirm');
        if (!btn) return;
        btn.addEventListener('click', async () => {
            if (!deleteProgramTargetId) return;
            const feedback = document.getElementById('program-delete-feedback');
            showFieldFeedback(feedback, '');
            setBtnLoading(btn, true);
            const res = await apiCall('DELETE', `/admin/programs/${deleteProgramTargetId}`);
            setBtnLoading(btn, false);
            if (!res.ok) return showFieldFeedback(feedback, res.message);
            deleteProgramTargetId = null;
            closeModal('program-delete-modal');
            showToast('Program deleted.', 'success');
            loadProgramList();
        });
    }

    // Registered once at module scope (not per-init) so navigating away and back
    // to the Programs section does not stack duplicate delegated listeners.
    document.body.addEventListener('click', (e) => {
        const btn = e.target.closest('[data-action="rpc"]');
        if (!btn || !btn.dataset.programId) return;
        const id = btn.dataset.programId;
        const cached = window.__programCache || [];
        const item = cached.find(i => String(i.program?.id || i.id) === id);
        const rpc = item?.rpc || {};

        document.getElementById('program-rpc-title').textContent = `RPC for Program ${id}`;
        document.getElementById('program-rpc-http').textContent = rpc.http_url || '—';
        document.getElementById('program-rpc-ws').textContent = rpc.ws_url || '—';
        document.getElementById('program-rpc-id').textContent = rpc.id || '—';

        const gotoBtn = document.getElementById('program-rpc-goto');
        if (gotoBtn) {
            const validRpc = rpc.id && rpc.id !== 0;
            gotoBtn.disabled = !validRpc;
            gotoBtn.onclick = validRpc ? () => {
                closeModal('program-rpc-modal');
                const rpcNav = document.querySelector('.nav-item[hx-get="fragments/rpcs.html"]');
                if (rpcNav) {
                    setActiveNav(rpcNav);
                    rpcNav.click();
                }
            } : null;
        }
        openModal('program-rpc-modal');
    });

    // Registered once at module scope so re-entering the Programs section doesn't stack listeners.
    document.body.addEventListener('click', async (e) => {
        const btn = e.target.closest('[data-action="toggle-active"]');
        if (!btn || !btn.dataset.programId) return;
        const id = btn.dataset.programId;
        const activate = btn.dataset.active !== 'true'; // currently inactive → activate, and vice-versa
        setBtnLoading(btn, true);
        const res = await apiCall('POST', `/admin/programs/${id}/${activate ? 'activate' : 'deactivate'}`);
        setBtnLoading(btn, false);
        if (!res.ok) return showToast(res.message, 'error');
        showToast(activate ? 'Program activated.' : 'Program deactivated.', 'success');
        loadProgramList();
    });

    window.umbra.openProgramEdit = openProgramEditById;

    // Dashboard section ---------------------------------------------------------
    function initDashboardSection() {
        loadDashboardStats();
        loadDashboardActivity();
        document.getElementById('dashboard-refresh')?.addEventListener('click', () => {
            loadDashboardStats();
            loadDashboardActivity();
        });
    }

    function formatCount(value) {
        const num = Number(value);
        return Number.isFinite(num) ? num.toLocaleString('en-US') : '—';
    }

    async function loadDashboardStats() {
        const setStat = (id, v) => { const el = document.getElementById(id); if (el) el.textContent = v; };
        const statIds = ['stat-programs', 'stat-rpcs', 'stat-slots', 'stat-transactions', 'stat-events', 'stat-accounts'];
        const root = document.getElementById('dash-progress-root');
        const count = document.getElementById('dash-progress-count');
        const empty = document.getElementById('dash-progress-empty');

        const res = await apiCall('GET', '/admin/stats');
        if (!res.ok) {
            statIds.forEach(id => setStat(id, '—'));
            if (root) root.innerHTML = '';
            if (count) count.textContent = 'Failed to load';
            if (empty) empty.hidden = false;
            showToast(res.message, 'error');
            return;
        }

        const stats = res.data || {};
        const est = stats.estimates || {};
        setStat('stat-programs', formatCount(stats.programs));
        setStat('stat-rpcs', formatCount(stats.rpcs));
        setStat('stat-slots', formatCount(est.slots));
        setStat('stat-transactions', formatCount(est.transactions));
        setStat('stat-events', formatCount(est.events));
        setStat('stat-accounts', formatCount(est.accounts));

        const programs = stats.per_program || [];
        if (count) count.textContent = `${programs.length} program${programs.length === 1 ? '' : 's'}`;
        if (!root) return;
        if (programs.length === 0) {
            root.innerHTML = '';
            if (empty) empty.hidden = false;
            return;
        }
        if (empty) empty.hidden = true;
        root.innerHTML = `<div class="dash-list">${programs.map(renderProgressRow).join('')}</div>`;
    }

    function renderProgressRow(p) {
        const name = p.name || `Program ${p.id}`;
        const slot = p.last_processed_slot;
        return `
            <div class="dash-row">
                <div class="dash-row-main">
                    <span class="dash-row-title">${escapeHtml(name)}</span>
                    <span class="dash-row-sub mono" title="${escapeHtml(p.address)}">${escapeHtml(p.address)}</span>
                </div>
                <span class="status-badge ${slot ? 'indexed' : 'pending'}">${slot ? `Slot ${escapeHtml(formatCount(slot))}` : 'Pending'}</span>
            </div>
        `;
    }

    async function loadDashboardActivity() {
        const root = document.getElementById('dash-activity-root');
        const count = document.getElementById('dash-activity-count');
        const empty = document.getElementById('dash-activity-empty');
        if (!root) return;

        const res = await apiCall('GET', `/transactions${buildQuery({ page: 1, per_page: 8 })}`);
        if (!res.ok) {
            root.innerHTML = '';
            if (count) count.textContent = 'Failed to load';
            if (empty) empty.hidden = false;
            showToast(res.message, 'error');
            return;
        }

        const rows = res.data?.data || [];
        if (count) count.textContent = `${formatCount(res.data?.total ?? rows.length)} total`;
        if (rows.length === 0) {
            root.innerHTML = '';
            if (empty) empty.hidden = false;
            return;
        }
        if (empty) empty.hidden = true;
        root.innerHTML = `<div class="dash-list">${rows.map(renderActivityRow).join('')}</div>`;
    }

    function shortSig(sig) {
        return sig && sig.length > 16 ? `${sig.slice(0, 8)}…${sig.slice(-8)}` : (sig || '');
    }

    function renderActivityRow(tx) {
        return `
            <div class="dash-row">
                <div class="dash-row-main">
                    <span class="dash-row-title mono" title="${escapeHtml(tx.signature || '')}">${escapeHtml(shortSig(tx.signature))}</span>
                    <span class="dash-row-sub">Slot ${escapeHtml(formatCount(tx.slot))}</span>
                </div>
                <span class="status-badge ${tx.success ? 'indexed' : 'failed'}">${tx.success ? 'Success' : 'Failed'}</span>
            </div>
        `;
    }

    // Shared program picker used by the read-API sections (pdas,
    // transactions, events). Reuses the admin program list and caches it.
    async function populateProgramSelect(selectId, { includeAll = false } = {}) {
        const select = document.getElementById(selectId);
        if (!select) return;
        const current = select.value;
        select.innerHTML = '';
        const placeholder = document.createElement('option');
        placeholder.value = '';
        placeholder.textContent = includeAll ? 'All programs' : 'Select a program…';
        placeholder.disabled = !includeAll;
        placeholder.selected = true;
        select.appendChild(placeholder);

        const res = await apiCall('GET', '/admin/programs');
        const list = res.ok ? (res.data?.data || []) : [];
        if (res.ok) window.__programCache = list;
        list.forEach(item => {
            const program = item.program || item;
            const option = document.createElement('option');
            option.value = program.id;
            option.textContent = `${program.id}: ${program.name || formatProgramAddress(program.address)}`;
            select.appendChild(option);
        });
        if (current) select.value = current;
    }

    // PDAs section --------------------------------------------------------------
    let pdasList = null;

    function initPdasSection() {
        populateProgramSelect('pdas-program');
        pdasList = createListSection({
            rootId: 'pdas-root', emptyId: 'pdas-empty', countId: 'pdas-count', pagerId: 'pdas-pager',
            endpoint: () => {
                const id = document.getElementById('pdas-program')?.value;
                return id ? `/programs/${id}/accounts` : null;
            },
            params: () => {
                const memcmp = document.getElementById('pdas-memcmp')?.value.trim();
                const dataSize = document.getElementById('pdas-datasize')?.value.trim();
                const p = {};
                if (memcmp) p.memcmp = memcmp;
                if (dataSize) p.dataSize = dataSize;
                return p;
            },
            renderRows: (rows) => `<div class="dash-list">${rows.map(renderAccountRow).join('')}</div>`,
            countText: (total) => `${formatCount(total)} account${total === 1 ? '' : 's'}`
        });

        const apply = () => pdasList.reload();
        document.getElementById('pdas-apply')?.addEventListener('click', apply);
        document.getElementById('pdas-program')?.addEventListener('change', apply);
        ['pdas-memcmp', 'pdas-datasize'].forEach(id => {
            document.getElementById(id)?.addEventListener('keydown', (e) => { if (e.key === 'Enter') apply(); });
        });

        pdasList.load();
    }

    function renderAccountRow(acc) {
        return `
            <div class="dash-row clickable" data-account="${escapeHtml(acc.address)}">
                <div class="dash-row-main">
                    <span class="dash-row-title mono" title="${escapeHtml(acc.address)}">${escapeHtml(acc.address)}</span>
                    <span class="dash-row-sub">${escapeHtml(formatCount(acc.lamports))} lamports</span>
                </div>
                <span class="status-badge indexed">Slot ${escapeHtml(formatCount(acc.slot))}</span>
            </div>
        `;
    }

    async function openAccountDetail(address) {
        const set = (id, v) => { const el = document.getElementById(id); if (el) el.textContent = v; };
        ['pdas-detail-address', 'pdas-detail-owner', 'pdas-detail-lamports', 'pdas-detail-slot',
         'pdas-detail-wv', 'pdas-detail-rent', 'pdas-detail-time'].forEach(id => set(id, '…'));
        set('pdas-detail-data', 'Loading…');
        openModal('pdas-detail-modal');

        const res = await apiCall('GET', `/accounts/${encodeURIComponent(address)}`);
        if (!res.ok) {
            closeModal('pdas-detail-modal');
            showToast(res.message, 'error');
            return;
        }
        const acc = res.data || {};
        set('pdas-detail-address', acc.address || address);
        set('pdas-detail-owner', acc.owner || '—');
        set('pdas-detail-lamports', formatCount(acc.lamports));
        set('pdas-detail-slot', formatCount(acc.slot));
        set('pdas-detail-wv', formatCount(acc.write_version));
        set('pdas-detail-rent', acc.rent_epoch != null ? formatCount(acc.rent_epoch) : '—');
        set('pdas-detail-time', acc.block_time != null ? new Date(acc.block_time * 1000).toISOString() : '—');

        const dataEl = document.getElementById('pdas-detail-data');
        if (dataEl) {
            dataEl.dataset.b64 = acc.data || '';
            dataEl.dataset.mode = 'base64';
            dataEl.textContent = acc.data || '(empty)';
        }
        const toggle = document.getElementById('pdas-detail-data-toggle');
        if (toggle) toggle.textContent = 'Show hex';
    }

    // Delegated once at module scope so re-swapping the section never stacks them.
    document.body.addEventListener('click', (e) => {
        const row = e.target.closest('.dash-row.clickable[data-account]');
        if (row) return openAccountDetail(row.dataset.account);

        const toggle = e.target.closest('#pdas-detail-data-toggle');
        if (toggle) {
            const dataEl = document.getElementById('pdas-detail-data');
            if (!dataEl) return;
            const b64 = dataEl.dataset.b64 || '';
            if (dataEl.dataset.mode === 'base64') {
                const bytes = b64 ? base64ToBytes(b64) : new Uint8Array();
                dataEl.textContent = bytes.length ? bytesToHex(bytes) : '(empty)';
                dataEl.dataset.mode = 'hex';
                toggle.textContent = 'Show base64';
            } else {
                dataEl.textContent = b64 || '(empty)';
                dataEl.dataset.mode = 'base64';
                toggle.textContent = 'Show hex';
            }
        }
    });

    // Transactions section ------------------------------------------------------
    let txList = null;

    function initTxSection() {
        populateProgramSelect('tx-program');
        txList = createListSection({
            rootId: 'tx-root', emptyId: 'tx-empty', countId: 'tx-count', pagerId: 'tx-pager',
            endpoint: () => {
                const id = document.getElementById('tx-program')?.value;
                return id ? `/programs/${id}/transactions` : null;
            },
            params: () => {
                const success = document.getElementById('tx-success')?.value;
                const from = document.getElementById('tx-from')?.value.trim();
                const to = document.getElementById('tx-to')?.value.trim();
                const p = {};
                if (success) p.success = success;
                if (from) p.from_slot = from;
                if (to) p.to_slot = to;
                if (document.getElementById('tx-correlated')?.classList.contains('on')) p.correlated = 'true';
                return p;
            },
            renderRows: (rows) => `<div class="dash-list">${rows.map(renderTxRow).join('')}</div>`,
            countText: (total) => `${formatCount(total)} transaction${total === 1 ? '' : 's'}`
        });

        const apply = () => {
            const correlated = document.getElementById('tx-correlated')?.classList.contains('on');
            const from = document.getElementById('tx-from')?.value.trim();
            const to = document.getElementById('tx-to')?.value.trim();
            // the correlated view is a multi-hop scan; the API rejects it unbounded
            if (correlated && !from && !to) {
                showToast('Correlated view needs a From or To slot bound.', 'error');
                return;
            }
            txList.reload();
        };
        document.getElementById('tx-apply')?.addEventListener('click', apply);
        ['tx-program', 'tx-success'].forEach(id => document.getElementById(id)?.addEventListener('change', apply));
        document.getElementById('tx-correlated')?.addEventListener('click', (e) => {
            const btn = e.currentTarget;
            const on = !btn.classList.contains('on');
            btn.classList.toggle('on', on);
            btn.setAttribute('aria-checked', String(on));
            apply();
        });
        ['tx-from', 'tx-to'].forEach(id => {
            document.getElementById(id)?.addEventListener('keydown', (e) => { if (e.key === 'Enter') apply(); });
        });
        txList.load();
    }

    // Resolves a program id to its "id: name" label using the shared program cache.
    function programLabel(id) {
        const item = (window.__programCache || []).find(entry => (entry.program || entry).id === id);
        const program = item && (item.program || item);
        return program?.name ? `${id}: ${program.name}` : `Program ${id}`;
    }

    function renderTxRow(tx) {
        return `
            <div class="dash-row clickable" data-tx="${escapeHtml(tx.signature)}">
                <div class="dash-row-main">
                    <span class="dash-row-title mono" title="${escapeHtml(tx.signature)}">${escapeHtml(shortSig(tx.signature))}</span>
                    <span class="dash-row-sub">Slot ${escapeHtml(formatCount(tx.slot))}${tx.fee != null ? ` · ${escapeHtml(formatCount(tx.fee))} fee` : ''}</span>
                </div>
                <span class="status-badge ${tx.success ? 'indexed' : 'failed'}">${tx.success ? 'Success' : 'Failed'}</span>
            </div>
        `;
    }

    async function openTxDetail(signature) {
        const set = (id, v) => { const el = document.getElementById(id); if (el) el.textContent = v; };
        ['tx-detail-signature', 'tx-detail-payer', 'tx-detail-status', 'tx-detail-program',
         'tx-detail-slot', 'tx-detail-fee', 'tx-detail-time'].forEach(id => set(id, '…'));
        set('tx-detail-programs', '…');
        set('tx-detail-raw', 'Loading…');
        const errBlock = document.getElementById('tx-detail-error-block');
        if (errBlock) errBlock.hidden = true;
        openModal('tx-detail-modal');

        const res = await apiCall('GET', `/transactions/${encodeURIComponent(signature)}`);
        if (!res.ok) {
            closeModal('tx-detail-modal');
            showToast(res.message, 'error');
            return;
        }
        const tx = res.data || {};
        set('tx-detail-signature', tx.signature || signature);
        set('tx-detail-payer', tx.fee_payer || '—');
        set('tx-detail-status', tx.success ? 'Success' : 'Failed');
        set('tx-detail-program', tx.program_id != null ? tx.program_id : '—');
        const programsEl = document.getElementById('tx-detail-programs');
        if (programsEl) {
            const programs = tx.programs || [];
            programsEl.innerHTML = programs.length
                ? programs.map(id => `<span class="tx-program-chip${id === tx.program_id ? ' attributed' : ''}">${escapeHtml(programLabel(id))}</span>`).join('')
                : '<span class="rpc-detail-value">—</span>';
        }
        set('tx-detail-slot', formatCount(tx.slot));
        set('tx-detail-fee', tx.fee != null ? `${formatCount(tx.fee)} lamports` : '—');
        set('tx-detail-time', tx.block_time != null ? new Date(tx.block_time * 1000).toISOString() : '—');
        set('tx-detail-raw', tx.raw != null ? JSON.stringify(tx.raw, null, 2) : '(empty)');
        if (errBlock) {
            errBlock.hidden = tx.err == null;
            if (tx.err != null) set('tx-detail-error', JSON.stringify(tx.err, null, 2));
        }
    }

    document.body.addEventListener('click', (e) => {
        const row = e.target.closest('.dash-row.clickable[data-tx]');
        if (row) openTxDetail(row.dataset.tx);
    });

    // Events section ------------------------------------------------------------
    let eventsList = null;
    let eventsCache = [];

    // Leading 8 bytes of an event's base64 data, as hex — the Anchor-style
    // discriminator the list can also be filtered by.
    function discriminatorHex(b64) {
        if (!b64) return '';
        const bytes = base64ToBytes(b64);
        return bytesToHex(bytes.subarray(0, 8));
    }

    function initEventsSection() {
        populateProgramSelect('ev-program');
        eventsList = createListSection({
            rootId: 'ev-root', emptyId: 'ev-empty', countId: 'ev-count', pagerId: 'ev-pager',
            endpoint: () => {
                const id = document.getElementById('ev-program')?.value;
                return id ? `/programs/${id}/events` : null;
            },
            params: () => {
                const disc = document.getElementById('ev-discriminator')?.value.trim();
                const sig = document.getElementById('ev-signature')?.value.trim();
                const from = document.getElementById('ev-from')?.value.trim();
                const to = document.getElementById('ev-to')?.value.trim();
                const p = {};
                if (disc) p.discriminator = disc;
                if (sig) p.tx_signature = sig;
                if (from) p.from_slot = from;
                if (to) p.to_slot = to;
                return p;
            },
            renderRows: (rows) => {
                eventsCache = rows;
                return `<div class="dash-list">${rows.map(renderEventRow).join('')}</div>`;
            },
            countText: (total) => `${formatCount(total)} event${total === 1 ? '' : 's'}`
        });

        const apply = () => eventsList.reload();
        document.getElementById('ev-apply')?.addEventListener('click', apply);
        document.getElementById('ev-program')?.addEventListener('change', apply);
        ['ev-discriminator', 'ev-signature', 'ev-from', 'ev-to'].forEach(id => {
            document.getElementById(id)?.addEventListener('keydown', (e) => { if (e.key === 'Enter') apply(); });
        });
        eventsList.load();
    }

    function renderEventRow(ev, idx) {
        const disc = discriminatorHex(ev.data);
        return `
            <div class="dash-row clickable" data-event-idx="${idx}">
                <div class="dash-row-main">
                    <span class="dash-row-title mono">${disc ? escapeHtml(disc) : '(no data)'}</span>
                    <span class="dash-row-sub mono" title="${escapeHtml(ev.tx_signature || '')}">${escapeHtml(shortSig(ev.tx_signature))} · #${escapeHtml(ev.log_index)}</span>
                </div>
                <span class="status-badge indexed">Slot ${escapeHtml(formatCount(ev.slot))}</span>
            </div>
        `;
    }

    function openEventDetail(ev) {
        const set = (id, v) => { const el = document.getElementById(id); if (el) el.textContent = v; };
        set('ev-detail-signature', ev.tx_signature || '—');
        set('ev-detail-discriminator', discriminatorHex(ev.data) || '—');
        set('ev-detail-logindex', ev.log_index);
        set('ev-detail-slot', formatCount(ev.slot));
        set('ev-detail-time', ev.block_time != null ? new Date(ev.block_time * 1000).toISOString() : '—');

        const dataEl = document.getElementById('ev-detail-data');
        if (dataEl) {
            dataEl.dataset.b64 = ev.data || '';
            dataEl.dataset.mode = 'base64';
            dataEl.textContent = ev.data || '(empty)';
        }
        const toggle = document.getElementById('ev-detail-data-toggle');
        if (toggle) toggle.textContent = 'Show hex';
        openModal('ev-detail-modal');
    }

    document.body.addEventListener('click', (e) => {
        const row = e.target.closest('.dash-row.clickable[data-event-idx]');
        if (row) {
            const ev = eventsCache[Number(row.dataset.eventIdx)];
            if (ev) openEventDetail(ev);
            return;
        }
        const toggle = e.target.closest('#ev-detail-data-toggle');
        if (toggle) {
            const dataEl = document.getElementById('ev-detail-data');
            if (!dataEl) return;
            const b64 = dataEl.dataset.b64 || '';
            if (dataEl.dataset.mode === 'base64') {
                const bytes = b64 ? base64ToBytes(b64) : new Uint8Array();
                dataEl.textContent = bytes.length ? bytesToHex(bytes) : '(empty)';
                dataEl.dataset.mode = 'hex';
                toggle.textContent = 'Show base64';
            } else {
                dataEl.textContent = b64 || '(empty)';
                dataEl.dataset.mode = 'base64';
                toggle.textContent = 'Show hex';
            }
        }
    });

    // Monitoring section --------------------------------------------------------
    const STREAM_LOG_CAP = 200;
    let monEventSource = null;
    let monStreamType = 'accounts';
    let monStreamProgram = null;

    function initMonitoringSection() {
        populateProgramSelect('mon-stream-program');
        loadMonitoringStats();
        document.getElementById('monitoring-refresh')?.addEventListener('click', loadMonitoringStats);
        document.getElementById('mon-stream-toggle')?.addEventListener('click', toggleStream);
        document.getElementById('mon-stream-clear')?.addEventListener('click', clearStreamLog);
        // A section re-swap discards the old log DOM, so drop any live stream too.
        disconnectStream();
    }

    async function loadMonitoringStats() {
        const root = document.getElementById('mon-progress-root');
        const count = document.getElementById('mon-progress-count');
        const empty = document.getElementById('mon-progress-empty');
        if (!root) return;

        root.innerHTML = `<div class="skeleton-table" aria-busy="true"><div class="skeleton-row"></div><div class="skeleton-row"></div></div>`;
        if (empty) empty.hidden = true;
        if (count) count.textContent = 'Loading…';

        const res = await apiCall('GET', '/admin/stats');
        if (!res.ok) {
            root.innerHTML = '';
            if (count) count.textContent = 'Failed to load';
            if (empty) empty.hidden = false;
            showToast(res.message, 'error');
            return;
        }

        const programs = res.data?.per_program || [];
        if (count) count.textContent = `${programs.length} program${programs.length === 1 ? '' : 's'}`;
        if (programs.length === 0) {
            root.innerHTML = '';
            if (empty) empty.hidden = false;
            return;
        }
        if (empty) empty.hidden = true;
        // Bars are scaled against the furthest program: without a chain tip there
        // is no absolute target, so this shows relative fleet progress honestly.
        const maxSlot = programs.reduce((m, p) => Math.max(m, p.last_processed_slot || 0), 0);
        root.innerHTML = `<div class="progress-list">${programs.map(p => renderProgressBar(p, maxSlot)).join('')}</div>`;
    }

    function renderProgressBar(p, maxSlot) {
        const name = p.name || `Program ${p.id}`;
        const last = p.last_processed_slot || 0;
        const pct = maxSlot > 0 ? Math.round((last / maxSlot) * 100) : 0;
        const started = p.start_slot != null ? `from ${formatCount(p.start_slot)}` : 'not started';
        const when = p.last_processed_at || 'never';
        return `
            <div class="progress-item">
                <div class="progress-head">
                    <span class="progress-name">${escapeHtml(name)}</span>
                    <span class="progress-slot mono">${last ? `Slot ${escapeHtml(formatCount(last))}` : 'Pending'}</span>
                </div>
                <div class="progress-track"><div class="progress-fill" style="width:${pct}%"></div></div>
                <div class="progress-meta">
                    <span class="mono" title="${escapeHtml(p.address)}">${escapeHtml(p.address)}</span>
                    <span>${escapeHtml(started)} · ${escapeHtml(when)}</span>
                </div>
            </div>
        `;
    }

    function toggleStream() {
        if (monEventSource) disconnectStream();
        else connectStream();
    }

    function connectStream() {
        const id = document.getElementById('mon-stream-program')?.value;
        const type = document.getElementById('mon-stream-type')?.value || 'accounts';
        if (!id) return showToast('Pick a program to stream.', 'info');

        disconnectStream();
        monStreamProgram = id;
        monStreamType = type;
        document.getElementById('mon-stream-empty')?.setAttribute('hidden', '');
        setStreamStatus('pending', 'Connecting…');

        const es = new EventSource(`/sse/programs/${id}/${type}`);
        monEventSource = es;
        es.onopen = () => setStreamStatus('indexed', 'Live');
        // EventSource reconnects on its own; surface the gap while it retries.
        es.onerror = () => setStreamStatus('failed', 'Reconnecting…');
        es.addEventListener('account', (e) => pushStreamRow('account', safeJson(e.data)));
        es.addEventListener('event', (e) => pushStreamRow('event', safeJson(e.data)));
        es.addEventListener('transaction', (e) => pushStreamRow('transaction', safeJson(e.data)));
        es.addEventListener('resync', (e) => handleResync(safeJson(e.data)));
        updateStreamButton();
    }

    function disconnectStream() {
        if (monEventSource) { monEventSource.close(); monEventSource = null; }
        setStreamStatus('pending', 'Idle');
        updateStreamButton();
    }

    function updateStreamButton() {
        const btn = document.getElementById('mon-stream-toggle');
        if (btn) btn.textContent = monEventSource ? 'Disconnect' : 'Connect';
    }

    function setStreamStatus(state, text) {
        const pill = document.getElementById('mon-stream-status');
        if (!pill) return;
        pill.className = `status-badge ${state}`;
        pill.textContent = text;
    }

    function safeJson(text) {
        try { return JSON.parse(text); } catch { return {}; }
    }

    function clearStreamLog() {
        const log = document.getElementById('mon-stream-log');
        if (log) log.innerHTML = '';
        const empty = document.getElementById('mon-stream-empty');
        if (empty && !monEventSource) empty.hidden = false;
    }

    function appendStreamHtml(html) {
        const log = document.getElementById('mon-stream-log');
        if (!log) return;
        document.getElementById('mon-stream-empty')?.setAttribute('hidden', '');
        log.insertAdjacentHTML('afterbegin', html);
        while (log.children.length > STREAM_LOG_CAP) log.lastElementChild.remove();
    }

    function pushStreamRow(kind, payload, backfill = false) {
        const slot = `Slot ${escapeHtml(formatCount(payload.slot))}`;
        let title, sub;
        if (kind === 'account') {
            title = escapeHtml(payload.address || '—');
            sub = `wv ${escapeHtml(formatCount(payload.write_version))}`;
        } else if (kind === 'transaction') {
            title = escapeHtml(shortSig(payload.signature));
            const status = payload.success ? 'success' : 'failed';
            sub = payload.fee != null ? `${status} · fee ${escapeHtml(formatCount(payload.fee))}` : status;
        } else {
            const disc = discriminatorHex(payload.data);
            title = disc ? escapeHtml(disc) : '(no data)';
            sub = escapeHtml(shortSig(payload.signature || payload.tx_signature));
        }
        const tag = backfill ? `${kind} · backfill` : kind;
        appendStreamHtml(`
            <div class="stream-row">
                <span class="stream-kind ${kind}${backfill ? ' backfill' : ''}">${escapeHtml(tag)}</span>
                <div class="dash-row-main">
                    <span class="dash-row-title mono">${title}</span>
                    <span class="dash-row-sub mono">${sub}</span>
                </div>
                <span class="stream-slot mono">${slot}</span>
            </div>
        `);
    }

    // A lagged stream sends a `resync` frame with the last slot it delivered; for
    // events/transactions we re-pull the missed window from the read API so nothing is
    // silently dropped — the account stream has no history to replay and just resumes live.
    async function handleResync(payload) {
        appendStreamHtml(`<div class="stream-row resync"><span class="stream-kind resync">resync</span><div class="dash-row-main"><span class="dash-row-title">Missed ${escapeHtml(formatCount(payload.missed))} change${payload.missed === 1 ? '' : 's'}</span><span class="dash-row-sub">re-pulling since slot ${escapeHtml(formatCount(payload.since_slot))}</span></div></div>`);
        if (payload.since_slot == null || !monStreamProgram) return;

        const id = monStreamProgram;
        if (monStreamType === 'events') {
            const res = await apiCall('GET', `/programs/${id}/events${buildQuery({ since_slot: payload.since_slot, per_page: 50 })}`);
            if (res.ok) (res.data?.data || []).slice().reverse().forEach(ev => pushStreamRow('event', ev, true));
        } else if (monStreamType === 'transactions') {
            const res = await apiCall('GET', `/programs/${id}/transactions${buildQuery({ since_slot: payload.since_slot, per_page: 50 })}`);
            if (res.ok) (res.data?.data || []).slice().reverse().forEach(t => pushStreamRow('transaction', t, true));
        }
    }

    document.body.addEventListener('htmx:afterSwap', (evt) => {
        const trigger = evt.detail.requestConfig?.trigger;
        if (trigger?.classList.contains('nav-item')) setActiveNav(trigger);
        Prefs.reflect();
        const swapped = evt.detail.target;
        if (swapped?.matches?.('[data-section="rpcs"]') || swapped?.querySelector?.('[data-section="rpcs"]')) {
            initRpcSection();
        }
        if (swapped?.matches?.('[data-section="programs"]') || swapped?.querySelector?.('[data-section="programs"]')) {
            initProgramsSection();
        }
        if (swapped?.matches?.('[data-section="dashboard"]') || swapped?.querySelector?.('[data-section="dashboard"]')) {
            initDashboardSection();
        }
        if (swapped?.matches?.('[data-section="pdas"]') || swapped?.querySelector?.('[data-section="pdas"]')) {
            initPdasSection();
        }
        if (swapped?.matches?.('[data-section="transactions"]') || swapped?.querySelector?.('[data-section="transactions"]')) {
            initTxSection();
        }
        if (swapped?.matches?.('[data-section="events"]') || swapped?.querySelector?.('[data-section="events"]')) {
            initEventsSection();
        }
        const isMonitoring = swapped?.matches?.('[data-section="monitoring"]') || swapped?.querySelector?.('[data-section="monitoring"]');
        if (isMonitoring) {
            initMonitoringSection();
        } else if (monEventSource) {
            // Leaving monitoring: tear the live stream down so it never lingers.
            disconnectStream();
        }
    });

});
