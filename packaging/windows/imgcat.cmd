@echo off
rem NovaTerm — affiche une image dans le terminal. Permet d'écrire simplement
rem « imgcat photo.png » depuis cmd (PowerShell définit sa propre fonction).
rem %~dp0 = dossier de ce script, donc novaterm-cli.exe installé à côté.
"%~dp0novaterm-cli.exe" --imgcat %*
