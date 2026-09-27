if vim.g.skwd_theme_loaded then
    return
end
vim.g.skwd_theme_loaded = true

local api, uv = vim.api, vim.uv or vim.loop
local loader = debug.getinfo(1, 'S').source:sub(2)
local root = vim.fn.fnamemodify(loader, ':h:h:h')
local palette = root .. '/skwd-colors.json'
local original, applied, truecolor
local watchers = {}
local group = api.nvim_create_augroup('SkwdTheme', { clear = true })

local function restore()
    if not original then
        return
    end
    for name in pairs(applied) do
        api.nvim_set_hl(0, name, original[name] or {})
    end
    vim.o.termguicolors = truecolor
    original, applied, truecolor = nil, nil, nil
end

local function refresh()
    local ok, lines = pcall(vim.fn.readfile, loader)
    if not ok or not table.concat(lines, '\n'):find('-- Skwd app theme', 1, true) then
        restore()
        return
    end
    ok, lines = pcall(vim.fn.readfile, palette)
    if not ok then
        restore()
        return
    end
    local decoded, highlights = pcall(vim.json.decode, table.concat(lines, '\n'))
    if not decoded or type(highlights) ~= 'table' then
        return
    end
    local current = api.nvim_get_hl(0, { link = true })
    local touched = {}
    local valid = pcall(function()
        for name, values in pairs(highlights) do
            if type(name) ~= 'string' or type(values) ~= 'table' then
                error('Invalid Neovim highlight mapping')
            end
            touched[name] = true
            api.nvim_set_hl(0, name, values)
        end
    end)
    if not valid then
        for name in pairs(touched) do
            api.nvim_set_hl(0, name, current[name] or {})
        end
        return
    end
    if not original then
        original, truecolor = current, vim.o.termguicolors
    end
    for name in pairs(applied or {}) do
        if not touched[name] then
            api.nvim_set_hl(0, name, original[name] or {})
        end
    end
    applied = touched
    vim.o.termguicolors = true
    vim.cmd.redraw()
end

local queued = false
local function schedule()
    if queued then
        return
    end
    queued = true
    vim.schedule(function()
        queued = false
        refresh()
    end)
end

for _, directory in ipairs({ root, vim.fn.fnamemodify(loader, ':h') }) do
    local watcher = uv.new_fs_event()
    if watcher then
        local started = watcher:start(directory, {}, function(err, filename)
            if not err and (not filename or filename == 'skwd-colors.json' or filename == 'skwd.lua') then
                schedule()
            end
        end)
        if started then
            table.insert(watchers, watcher)
        else
            watcher:close()
        end
    end
end
api.nvim_create_autocmd('ColorScheme', {
    group = group,
    callback = function()
        if original then
            original = api.nvim_get_hl(0, { link = true })
        end
        schedule()
    end,
})
api.nvim_create_autocmd('VimLeavePre', {
    group = group,
    callback = function()
        for _, watcher in ipairs(watchers) do
            watcher:stop()
            watcher:close()
        end
    end,
})
refresh()
