import os, sys
WURZEL = r'E:\GitHub\Projekte\SC-BP-Watcher'
sys.path.insert(0, WURZEL); sys.path.insert(0, os.path.join(WURZEL, 'tools'))
import unsichtbar
unsichtbar.sicherstellen(messend=True)
import tkinter as tk
from scbp import fields
HINWEIS = 'Bauplan oder Auftrag suchen'
w = tk.Tk(); w.geometry('400x100+30+30')
var = tk.StringVar()
f = tk.Entry(w, textvariable=var); f.pack()
steht = fields.hinweis(f, var, HINWEIS)
w.update()
ok = []
def p(b, t): ok.append((b, t)); print('  [%s] %s' % ('ok' if b else 'XX', t))
p(f.get() == HINWEIS, 'Start: Hinweis steht')
p(var.get() == '', 'Start: Variable leer')
# Tastendruck
f.focus_set(); w.update()
f.event_generate('<Key>', keysym='a'); w.update()
# ⚠ Erwartet wird NICHT „leer": Das getippte Zeichen steht ja drin. Gefragt
# ist nur, dass der Hinweis weg ist — sonst schriebe der Nutzer mitten hinein.
p(HINWEIS not in f.get(),
  'Tastendruck raeumt weg und holt ihn NICHT zurueck (%r)' % f.get())
p(not steht(), 'Zustand sagt: Hinweis aus')
# Programm setzt die Suche
var.set('titan'); w.update()
p(f.get() == 'titan', 'programmatisch gesetzt -> steht im Feld (%r)' % f.get())
# Programm leert die Suche (das Kreuz)
var.set(''); w.update()
p(f.get() == HINWEIS, 'geleert -> Hinweis sofort zurueck (%r)' % f.get())
p(var.get() == '', 'und die Variable bleibt leer')
# aus dem Hinweis heraus direkt setzen
var.set('Ezra'); w.update()
p(f.get() == 'Ezra', 'aus dem Hinweis heraus gesetzt (%r)' % f.get())
w.destroy()
print('\n  %d von %d' % (sum(1 for b,_ in ok if b), len(ok)))
sys.exit(0 if all(b for b,_ in ok) else 1)
