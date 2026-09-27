import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parents[2]
NVIM = shutil.which('nvim')


@unittest.skipUnless(NVIM, 'requires Neovim')
class NeovimTheme(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='nv-')
        self.root = Path(self.directory.name)
        self.config = self.root / 'config/nvim'
        self.loader = self.config / 'after/plugin/skwd.lua'
        self.loader.parent.mkdir(parents=True)
        self.loader.write_text('-- Skwd app theme\n' + (ROOT / 'data/app-themes/neovim.lua').read_text() + '\n-- End Skwd app theme\n')
        (self.config / 'init.lua').write_text("vim.o.termguicolors = false\nvim.api.nvim_set_hl(0, 'Normal', { fg = '#112233', bg = '#223344' })\nvim.api.nvim_set_hl(0, 'Comment', { link = 'Error' })\n")
        self.output = self.config / 'skwd-colors.json'
        self.publish('#abcdef')
        self.socket = str(self.root / 's')
        env = dict(os.environ, HOME=str(self.root), XDG_CONFIG_HOME=str(self.root / 'config'), XDG_DATA_HOME=str(self.root / 'data'), XDG_STATE_HOME=str(self.root / 'state'), XDG_CACHE_HOME=str(self.root / 'cache'), NVIM_APPNAME='nvim')
        self.log = open(self.root / 'stderr', 'w+')
        self.child = subprocess.Popen([NVIM, '--headless', '--listen', self.socket], env=env, stdin=subprocess.DEVNULL, stdout=self.log, stderr=self.log)
        self.addCleanup(self.stop)
        self.wait(lambda: Path(self.socket).exists())
        self.wait(lambda: self.highlight('Normal').get('fg') == 0xabcdef)

    def stop(self):
        self.child.terminate()
        self.child.wait(timeout=5)
        self.log.close()
        self.directory.cleanup()

    def expr(self, expression):
        return subprocess.check_output([NVIM, '--server', self.socket, '--remote-expr', expression], text=True, timeout=5).strip()

    def highlight(self, name):
        return json.loads(self.expr("json_encode(nvim_get_hl(0, {'name': '" + name + "', 'link': v:true}))"))

    def wait(self, predicate):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if predicate():
                return
            time.sleep(.02)
        self.log.flush()
        self.log.seek(0)
        self.fail('Neovim did not reach expected state: ' + self.log.read())

    def publish(self, color):
        template = (ROOT / 'data/app-themes/neovim.json').read_text()
        self.write_palette(re.sub(r'\{\{[^}]+\}\}', color, template))

    def write_palette(self, text):
        staged = self.output.with_suffix('.next')
        staged.write_text(text)
        staged.replace(self.output)

    def test_live_atomic_updates_disable_and_reenable_restore_original(self):
        self.assertEqual(self.expr('&termguicolors'), '1')
        self.publish('#123456')
        self.wait(lambda: self.highlight('Normal').get('fg') == 0x123456)
        self.output.unlink()
        self.wait(lambda: self.highlight('Normal').get('fg') == 0x112233)
        self.assertEqual(self.highlight('Comment'), {'link': 'Error'})
        self.assertEqual(self.expr('&termguicolors'), '0')
        self.publish('#fedcba')
        self.wait(lambda: self.highlight('Normal').get('fg') == 0xfedcba)
        loader = self.loader.read_text()
        self.loader.unlink()
        self.wait(lambda: self.highlight('Normal').get('fg') == 0x112233)
        self.assertEqual(self.expr('&termguicolors'), '0')
        self.output.unlink()
        self.publish('#13579b')
        self.loader.write_text(loader)
        self.wait(lambda: self.highlight('Normal').get('fg') == 0x13579b)
        self.output.unlink()
        self.loader.unlink()
        self.wait(lambda: self.highlight('Normal').get('fg') == 0x112233)

    def test_malformed_palette_preserves_current_theme_and_removed_groups_restore(self):
        self.write_palette('{invalid')
        time.sleep(.1)
        self.assertEqual(self.highlight('Normal')['fg'], 0xabcdef)
        self.write_palette(json.dumps({'Normal': {'fg': '#654321'}}))
        self.wait(lambda: self.highlight('Normal').get('fg') == 0x654321)
        self.assertEqual(self.highlight('Comment'), {'link': 'Error'})
        self.write_palette(json.dumps({'Normal': {'fg': '#123456'}, 'Bad': {'invalid_key': True}}))
        time.sleep(.1)
        self.assertEqual(self.highlight('Normal')['fg'], 0x654321)

    def test_external_colorscheme_becomes_restore_baseline(self):
        self.expr("execute('colorscheme default')")
        self.wait(lambda: self.highlight('Normal').get('fg') == 0xabcdef)
        self.output.unlink()
        self.wait(lambda: self.highlight('Normal').get('fg') != 0xabcdef)
        self.assertEqual(self.expr('g:colors_name'), 'default')
        self.assertEqual(self.expr('&termguicolors'), '0')


if __name__ == '__main__':
    unittest.main()
