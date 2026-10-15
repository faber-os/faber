import { describe, expect, it } from 'vitest';
import {
  baseName,
  breadcrumbs,
  childPath,
  extension,
  isAffected,
  isInTrash,
  parentPath,
  replayPortFor,
  splitPath,
  stem,
  uniqueName,
} from './filePaths';
import { back, canGoBack, canGoForward, forward, navigate, startHistory } from './history';

describe('paths', () => {
  it('splits a path into its root and folders', () => {
    expect(splitPath('home:/Desktop/notes.txt')).toEqual({
      root: 'home',
      segments: ['Desktop', 'notes.txt'],
    });
    expect(splitPath('home:/')).toEqual({ root: 'home', segments: [] });
    expect(splitPath('machine:/C:/Users')).toEqual({ root: 'machine', segments: ['C:', 'Users'] });
  });

  it('joins a child onto a folder', () => {
    expect(childPath('home:/', 'Desktop')).toBe('home:/Desktop');
    expect(childPath('home:/Desktop', 'a.txt')).toBe('home:/Desktop/a.txt');
  });

  it('finds the parent, or null at the root', () => {
    expect(parentPath('home:/Desktop/a.txt')).toBe('home:/Desktop');
    expect(parentPath('home:/Desktop')).toBe('home:/');
    expect(parentPath('home:/')).toBeNull();
  });

  it('gives the name, stem and lowercase extension', () => {
    expect(baseName('home:/Recordings/Bench.FBCAP')).toBe('Bench.FBCAP');
    expect(baseName('home:/')).toBe('');
    expect(extension('Bench.FBCAP')).toBe('fbcap');
    expect(extension('README')).toBe('');
    expect(extension('.profile')).toBe('');
    expect(stem('ld19-sample.fbcap')).toBe('ld19-sample');
  });

  it('builds breadcrumbs from the root label', () => {
    expect(breadcrumbs('home:/Documents/robot', 'Home')).toEqual([
      { label: 'Home', path: 'home:/' },
      { label: 'Documents', path: 'home:/Documents' },
      { label: 'robot', path: 'home:/Documents/robot' },
    ]);
  });

  it('knows Trash paths', () => {
    expect(isInTrash('home:/Trash/a.txt')).toBe(true);
    expect(isInTrash('home:/Trash')).toBe(false);
    expect(isInTrash('home:/Documents/Trash/a')).toBe(false);
  });

  it('names the replay port of a recording', () => {
    expect(replayPortFor('home:/Recordings/ld19-sample.fbcap')).toBe('replay:ld19-sample');
    expect(replayPortFor('home:/Documents/ld19-sample.fbcap')).toBeNull();
    expect(replayPortFor('home:/Recordings/notes.txt')).toBeNull();
  });

  it('picks a name that is not taken yet', () => {
    expect(uniqueName(['a.txt'], 'New folder')).toBe('New folder');
    expect(uniqueName(['New folder', 'New folder 2'], 'New folder')).toBe('New folder 3');
    expect(uniqueName(['New text file.txt'], 'New text file.txt')).toBe('New text file 2.txt');
  });

  it('says whether a files-changed message affects a folder, "<root>:/" meaning everything', () => {
    expect(isAffected('home:/Desktop', ['home:/Desktop'])).toBe(true);
    expect(isAffected('home:/Desktop', ['home:/Trash'])).toBe(false);
    expect(isAffected('home:/Desktop/x', ['home:/'])).toBe(true);
    expect(isAffected('machine:/C:', ['home:/'])).toBe(false);
  });
});

describe('history', () => {
  it('goes back and forward like a browser', () => {
    let h = startHistory('home:/Desktop');
    expect(canGoBack(h)).toBe(false);
    h = navigate(h, 'home:/Documents');
    h = navigate(h, 'home:/Documents/robot');
    h = back(h);
    expect(h.current).toBe('home:/Documents');
    expect(canGoForward(h)).toBe(true);
    h = back(h);
    expect(h.current).toBe('home:/Desktop');
    expect(canGoBack(h)).toBe(false);
    h = forward(h);
    expect(h.current).toBe('home:/Documents');
  });

  it('clears forward when going somewhere new', () => {
    let h = navigate(navigate(startHistory('a:/'), 'a:/b'), 'a:/c');
    h = navigate(back(h), 'a:/d');
    expect(canGoForward(h)).toBe(false);
    expect(back(h).current).toBe('a:/b');
  });

  it('ignores going to the folder already shown, and back/forward at the ends', () => {
    const h = startHistory('a:/');
    expect(navigate(h, 'a:/')).toBe(h);
    expect(back(h)).toBe(h);
    expect(forward(h)).toBe(h);
  });
});
