# -*- coding: utf-8 -*-
"""Prüfungen für `.github/scripts/beta_masken.py`: python tools/beta_masken_tests.py"""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / '.github' / 'scripts'))
import beta_masken  # noqa: E402

CODE = 'QX4M-K7QM-2XRA-9FHD-W3TB'


class Masken(unittest.TestCase):
    def test_every_form_of_a_code_and_the_name_is_masked(self):
        zeilen, fehler = beta_masken.masken([{'tester': 'Name', 'code': CODE.lower()}])
        self.assertEqual(fehler, [])
        self.assertEqual(sorted(zeilen), sorted(['::add-mask::' + CODE.replace('-', ''), '::add-mask::' + CODE,
                                                 '::add-mask::Name']))

    def test_o_and_i_are_read_as_the_program_reads_them(self):
        self.assertEqual(beta_masken.normal('qx4m k7qm 2xra 9fhd w3tb'), CODE.replace('-', ''))
        self.assertEqual(beta_masken.normal('OIL0-0000-0000-0000-0000'), '0110' + '0' * 16)

    def test_a_list_with_a_wrong_entry_is_refused_and_names_only_the_place(self):
        zeilen, fehler = beta_masken.masken([{'code': CODE}, {'tester': 'Geheim', 'code': 'zu kurz'}])
        self.assertEqual(fehler, ['Eintrag 2: kein Code'])
        self.assertFalse(any('Geheim' in f or 'kurz' in f for f in fehler))

    def test_two_codes_with_the_same_first_four_characters_are_refused(self):
        _, fehler = beta_masken.masken([{'code': CODE}, {'code': 'QX4M-0000-0000-0000-0000'}])
        self.assertEqual(len(fehler), 1)

    def test_an_empty_or_odd_list_is_refused(self):
        for liste in ([], {}, 'x', [1]):
            self.assertTrue(beta_masken.masken(liste)[1], liste)


if __name__ == '__main__':
    unittest.main()
