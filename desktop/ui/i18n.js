// UI strings for the toolbar and Settings. The tray menu lives in
// app/src/i18n.rs. To add a language: add it to LANGUAGES, give it a table in
// STRINGS (missing keys fall back to English), and add it to i18n.rs.
(function () {
  const LANGUAGES = [
    ['en', 'English'],
    ['vi', 'Tiếng Việt'],
  ];

  const STRINGS = {
    en: {
      tagline: 'A tiny hand on a stick that points at things on your screen.',
      powerOn: 'Turn pointer on',
      powerOff: 'Turn pointer off',
      unsupported: "The pointer overlay isn't available on this platform yet. Settings still work.",
      welcomeHtml:
        '<p><strong>Welcome!</strong> TinyHand lives in the system tray, next to the clock.</p><ul>' +
        '<li>Click the tray icon or press <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd> to turn the pointer on or off.</li>' +
        '<li>Click to tap. Nothing underneath gets clicked. Hold and drag to draw.</li>' +
        '<li>Hold <kbd>{key}</kbd> to click things for real. <kbd>Esc</kbd> turns the pointer off.</li></ul>',
      gotIt: 'Got it',
      pointer: 'Pointer',
      inToolbar: 'In your toolbar',
      count: '{n} of {max}',
      reset: 'Reset',
      stripLabel: 'Pointers in your toolbar',
      stripHint: 'Drag to reorder. Hover and click × to remove. Click one to use it now.',
      allPointers: 'All pointers',
      size: 'Size',
      shadow: 'Shadow',
      taps: 'Taps',
      handSound: 'Sound when a hand taps',
      laserSound: 'Sound when the laser dot taps',
      tapSound: 'Tap sound',
      play: 'Play',
      interact: 'Hold to click for real',
      interactHint: "While the pointer is on, apps never see this key, so {key}+click is a plain click (no Chrome download). Shortcuts like {key}+Tab still work.",
      off: 'Off',
      drawing: 'Drawing',
      inkColour: 'Ink colour',
      fadeAfter: 'Drawings fade after',
      sec1: '1 second',
      sec3: '3 seconds',
      sec5: '5 seconds',
      sec10: '10 seconds',
      never: 'Never (clear with Ctrl+Alt+X)',
      general: 'General',
      language: 'Language',
      login: 'Start TinyHand when I sign in',
      keyToggle: 'Turn the pointer on or off',
      keyToggleHtml: '<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd> or click the tray icon',
      keyClear: 'Clear the drawing',
      keyOff: 'Turn the pointer off',
      resetAll: 'Reset all settings',
      build: 'build',
      full: 'The toolbar is full. Remove one above to add {name}.',
      keepOne: 'Keep at least one pointer in the toolbar.',
      slotTitle: '{name} (Ctrl+← / Ctrl+→ to move)',
      removeName: 'Remove {name}',
      removeFromBar: 'Remove {name} from the toolbar',
      addToBar: 'Add {name} to the toolbar',
      useName: 'Use {name}',
      inBadge: '✓ In toolbar',
      removeShort: 'Remove',
      addShort: '+ Add to toolbar',
      inkTitle: 'Colour and thickness',
      soundTitle: 'Tap sound',
      exitTitle: 'Exit',
      pickTitle: 'Choose your language',
      pickHint: 'You can change this later in Settings.',
      welcomeHtmlMac:
        '<p><strong>Welcome!</strong> TinyHand lives in the menu bar.</p><ul>' +
        '<li>Click its icon or press <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd> to turn the pointer on or off. The first time, macOS asks for Accessibility, which lets TinyHand turn your clicks into taps.</li>' +
        '<li>Click to tap. Nothing underneath gets clicked. Hold and drag to draw.</li>' +
        '<li>Hold <kbd>{key}</kbd> to click things for real. <kbd>Esc</kbd> turns the pointer off.</li></ul>',
      keyToggleHtmlMac: '<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd> or click the menu bar icon',
      keyClearCombo: 'Ctrl+Alt+X',
      keyExitCombo: 'Esc',
      showHints: 'Explain toolbar buttons on hover',
      hintsOff: "Don't show again",
      hintsOffNote: 'You can turn this back on in Settings.',
      clearTitle: 'Clear everything',
      descInk: 'Choose the colour and thickness of your pen.',
      descClear: 'Removes the whole drawing.',
      descSound: 'Turns the tap sound on or off.',
      descExit: 'Turns the pointer off and clears drawings on the screen.',
      penSize: 'Thickness {n}',
    },
    vi: {
      tagline: 'Một bàn tay nhỏ trên que, chỉ vào mọi thứ trên màn hình của bạn.',
      powerOn: 'Bật con trỏ',
      powerOff: 'Tắt con trỏ',
      unsupported: 'Lớp con trỏ chưa có trên hệ điều hành này. Phần cài đặt vẫn dùng được.',
      welcomeHtml:
        '<p><strong>Chào mừng bạn!</strong> TinyHand nằm ở khay hệ thống, cạnh đồng hồ.</p><ul>' +
        '<li>Bấm biểu tượng ở khay hoặc nhấn <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd> để bật hoặc tắt con trỏ.</li>' +
        '<li>Bấm để chạm: những gì bên dưới sẽ không bị bấm. Giữ và kéo để vẽ.</li>' +
        '<li>Giữ <kbd>{key}</kbd> để bấm thật. Nhấn <kbd>Esc</kbd> để tắt con trỏ.</li></ul>',
      gotIt: 'Đã hiểu',
      pointer: 'Con trỏ',
      inToolbar: 'Trên thanh công cụ',
      count: '{n}/{max}',
      reset: 'Đặt lại',
      stripLabel: 'Các con trỏ trên thanh công cụ',
      stripHint: 'Kéo để sắp xếp. Rê chuột rồi bấm × để bỏ. Bấm vào một con trỏ để dùng ngay.',
      allPointers: 'Tất cả con trỏ',
      size: 'Kích thước',
      shadow: 'Bóng đổ',
      taps: 'Chạm',
      handSound: 'Phát âm thanh khi bàn tay chạm',
      laserSound: 'Phát âm thanh khi chấm laser chạm',
      tapSound: 'Âm thanh chạm',
      play: 'Nghe thử',
      interact: 'Giữ phím này để bấm thật',
      interactHint: 'Khi con trỏ đang bật, ứng dụng không nhận được phím này, nên {key}+click chỉ là một cú click bình thường (Chrome sẽ không tải xuống). Các phím tắt như {key}+Tab vẫn dùng được.',
      off: 'Tắt',
      drawing: 'Vẽ',
      inkColour: 'Màu mực',
      fadeAfter: 'Nét vẽ mờ dần sau',
      sec1: '1 giây',
      sec3: '3 giây',
      sec5: '5 giây',
      sec10: '10 giây',
      never: 'Không mờ (xóa bằng Ctrl+Alt+X)',
      general: 'Chung',
      language: 'Ngôn ngữ',
      login: 'Mở TinyHand khi đăng nhập',
      keyToggle: 'Bật hoặc tắt con trỏ',
      keyToggleHtml: '<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd> hoặc bấm biểu tượng ở khay',
      keyClear: 'Xóa nét vẽ',
      keyOff: 'Tắt con trỏ',
      resetAll: 'Đặt lại mọi cài đặt',
      build: 'bản dựng',
      full: 'Thanh công cụ đã đầy. Hãy bỏ bớt một con trỏ ở trên để thêm {name}.',
      keepOne: 'Cần giữ ít nhất một con trỏ trên thanh công cụ.',
      slotTitle: '{name} (Ctrl+← / Ctrl+→ để di chuyển)',
      removeName: 'Bỏ {name}',
      removeFromBar: 'Bỏ {name} khỏi thanh công cụ',
      addToBar: 'Thêm {name} vào thanh công cụ',
      useName: 'Dùng {name}',
      inBadge: '✓ Đã thêm',
      removeShort: 'Bỏ',
      addShort: '+ Thêm vào thanh',
      inkTitle: 'Màu và độ dày',
      soundTitle: 'Âm thanh chạm',
      exitTitle: 'Thoát',
      pickTitle: 'Chọn ngôn ngữ',
      pickHint: 'Bạn có thể đổi lại trong phần Cài đặt.',
      welcomeHtmlMac:
        '<p><strong>Chào mừng bạn!</strong> TinyHand nằm trên thanh menu.</p><ul>' +
        '<li>Bấm biểu tượng hoặc nhấn <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd> để bật hoặc tắt con trỏ. Lần đầu, macOS sẽ hỏi quyền Trợ năng để TinyHand biến cú bấm thành cú chạm.</li>' +
        '<li>Bấm để chạm: những gì bên dưới sẽ không bị bấm. Giữ và kéo để vẽ.</li>' +
        '<li>Giữ <kbd>{key}</kbd> để bấm thật. Nhấn <kbd>Esc</kbd> để tắt con trỏ.</li></ul>',
      keyToggleHtmlMac: '<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd> hoặc bấm biểu tượng trên thanh menu',
      showHints: 'Giải thích các nút khi rê chuột',
      hintsOff: 'Không hiện lại',
      hintsOffNote: 'Bạn có thể bật lại trong Cài đặt.',
      clearTitle: 'Xóa hết',
      descInk: 'Chọn màu và độ dày của bút.',
      descClear: 'Xóa toàn bộ nét vẽ.',
      descSound: 'Bật hoặc tắt âm thanh khi chạm.',
      descExit: 'Tắt con trỏ và xóa nét trên màn hình.',
      penSize: 'Độ dày {n}',
    },
  };

  const HANDS = {
    vi: {
      lavender: 'Oải hương', glove: 'Găng tay con trỏ', coral: 'San hô', plush: 'Thú bông', robot: 'Rô-bốt',
      pink: 'Kẹo cao su', blue: 'Bầu trời', mint: 'Bạc hà', lemon: 'Chanh vàng', black: 'Nửa đêm',
      chrome: 'Crôm', holo: 'Ánh hologram', fuzzy: 'Lông xù', wood: 'Gỗ', marble: 'Cẩm thạch',
      terrazzo: 'Đá mài', glass: 'Thủy tinh', jelly: 'Thạch', pastel: 'Pastel', pixel: 'Điểm ảnh',
      lowpoly: 'Đa giác', knit: 'Len đan', skeleton: 'Bộ xương', astro: 'Phi hành gia', space: 'Vũ trụ',
      monster: 'Quái vật', water: 'Nước', lava: 'Dung nham', grass: 'Cỏ', cactus: 'Xương rồng',
      choco: 'Sô-cô-la', cloud: 'Mây', mech: 'Người máy', neon: 'Đèn neon', leaf: 'Lá cây',
      cookie: 'Bánh gừng', laser: 'Chấm laser',
    },
  };

  const INKS = {
    vi: {
      match: 'Theo con trỏ', red: 'Đỏ', orange: 'Cam', yellow: 'Vàng', green: 'Xanh lá', blue: 'Xanh dương',
      purple: 'Tím', white: 'Trắng', black: 'Đen',
    },
  };

  const SOUNDS = {
    vi: { thock: 'Cộp', click: 'Tách', pop: 'Bốp', wood: 'Mõ gỗ', boop: 'Bíp', squeak: 'Chít' },
  };

  /** Best guess before the user has chosen: the system language. */
  function guess() {
    const langs = navigator.languages && navigator.languages.length ? navigator.languages : [navigator.language || 'en'];
    for (const l of langs) {
      const code = String(l).toLowerCase().split('-')[0];
      if (LANGUAGES.some(([c]) => c === code)) return code;
    }
    return 'en';
  }

  function lang(prefs) {
    const l = prefs && prefs.language;
    return l && STRINGS[l] ? l : guess();
  }

  const IS_MAC = /Mac/.test(navigator.platform || '');

  function t(l, key, vars) {
    const macKey = IS_MAC ? key + 'Mac' : key;
    let s = (STRINGS[l] && (STRINGS[l][macKey] ?? STRINGS[l][key])) ?? STRINGS.en[macKey] ?? STRINGS.en[key] ?? key;
    for (const [k, v] of Object.entries(vars || {})) s = s.split('{' + k + '}').join(v);
    return IS_MAC ? macNames(s) : s;
  }

  /** The same physical keys, as a Mac keyboard names them. */
  function macNames(s) {
    return s.replace(/Ctrl\+Alt\+/g, '⌃⌥').replace(/\bAlt\b/g, 'Option');
  }

  /** Fill [data-i18n] (text), [data-i18n-html] (markup), [data-i18n-title] and [data-i18n-aria]. */
  function apply(l, root, vars) {
    root = root || document;
    document.documentElement.lang = l;
    root.querySelectorAll('[data-i18n]').forEach(el => { el.textContent = t(l, el.dataset.i18n, vars); });
    root.querySelectorAll('[data-i18n-html]').forEach(el => { el.innerHTML = t(l, el.dataset.i18nHtml, vars); });
    root.querySelectorAll('[data-i18n-title]').forEach(el => { el.title = t(l, el.dataset.i18nTitle, vars); });
    root.querySelectorAll('[data-i18n-aria]').forEach(el => { el.setAttribute('aria-label', t(l, el.dataset.i18nAria, vars)); });
    if (IS_MAC) {
      root.querySelectorAll('kbd, option').forEach(el => {
        el.textContent = macNames(el.textContent).replace(/^Ctrl$/, '⌃ Control').replace(/^Option$/, el.tagName === 'KBD' ? '⌥' : 'Option');
      });
    }
  }

  const pick = (table, l, id, fallback) => (table[l] && table[l][id]) || fallback;

  window.I18n = {
    LANGUAGES,
    guess,
    lang,
    t,
    apply,
    hand: (l, id, fallback) => pick(HANDS, l, id, fallback),
    ink: (l, id, fallback) => pick(INKS, l, id, fallback),
    sound: (l, id, fallback) => pick(SOUNDS, l, id, fallback),
  };
})();
