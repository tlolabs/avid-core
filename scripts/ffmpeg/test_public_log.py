import json
import unittest
from unittest.mock import patch
from public_log import sanitize


class PublicLogTests(unittest.TestCase):
    def test_native_msys_and_json_paths_do_not_disclose_profile_or_checkout(self):
        env={'USERPROFILE':r'C:\Users\private-person',
             'GITHUB_WORKSPACE':r'D:\a\private-project\private-project',
             'RUNNER_TEMP':r'D:\a\_temp'}
        with patch.dict('os.environ',env):
            for value in [r'C:\Users\private-person\AppData\Local\Temp\file',
                          '/c/Users/private-person/AppData/Local/Temp/file',
                          'D:/a/private-project/private-project/src/file.rs',
                          r'D:\a\_temp\file']:
                for text in [value,json.dumps({'path':value})]:
                    cleaned=sanitize(text)
                    self.assertNotIn('private-person',cleaned)
                    self.assertNotIn('private-project',cleaned)
                    self.assertIn('<',cleaned)
                    if text.startswith('{'):json.loads(cleaned)
            self.assertEqual(sanitize('ffmpeg.exe exit=5 SHA256=abcdef'),
                             'ffmpeg.exe exit=5 SHA256=abcdef')
