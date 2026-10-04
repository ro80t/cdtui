@echo off
rem cdt for cmd.exe, which has no shell functions: the picker draws on stderr
rem and prints the chosen directory on stdout, and this file does the cd.
rem
rem Put this file in a directory that comes BEFORE %USERPROFILE%\.cargo\bin
rem in PATH. Within one directory cmd prefers .EXE over .CMD, so cdt.exe
rem would otherwise win the name and just print the path. Check: where cdt
rem
rem Quitting with q prints nothing, so the for loop runs zero times and the
rem current directory is left alone.
for /f "delims=" %%d in ('cdt.exe %*') do cd /d "%%d"
