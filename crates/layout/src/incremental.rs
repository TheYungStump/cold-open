//! Incremental layout (layout.md §15): after an edit, re-measure only the
//! elements that changed and repaginate only where pages can differ. The
//! result always equals laying the script out from scratch.

use std::collections::BTreeMap;

use coldopen_screenplay::{Script, TitlePage};

use crate::flow::{self, Block, Flow, FlowInput, Memo};
use crate::page::Page;
use crate::paginate::{Cursor, Paginator, Rules};
use crate::{Layout, Options, PageMeta, ValidFormat, no_progress, number_page, set_number_line};

/// Blocks a page may look past its last line when choosing a break: the
/// line that does not fit, a heading's two following lines and a
/// transition after the candidate. A page that ends further than this
/// before the first changed block cannot change.
const LOOKAHEAD: usize = 3;

/// What the last update did, for tests and performance reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateStats {
    /// Flow blocks before the first that changed.
    pub first_changed_block: usize,
    /// Pages kept from before the edit.
    pub reused_before: usize,
    /// Pages laid out again.
    pub laid_out: usize,
    /// Pages kept from after the edit, renumbered and shifted.
    pub reused_after: usize,
}

/// A layout that follows a script through its edits.
pub struct Incremental {
    format: ValidFormat,
    options: Options,
    rules: Rules,
    memo: Memo,
    flow: Flow,
    /// Elements in the script last laid out.
    elements: usize,
    title_page: TitlePage,
    layout: Layout,
    metas: Vec<PageMeta>,
    stats: UpdateStats,
}

fn same_block(a: &Block, b: &Block, shift: isize) -> bool {
    a.kind == b.kind
        && a.explicit_break == b.explicit_break
        && a.members.len() == b.members.len()
        && a.members
            .iter()
            .zip(&b.members)
            .all(|(x, y)| x.same_as(y, shift))
}

/// Blocks equal at the start, and equal (allowing for `shift`) at the end.
fn diff(old: &[Block], new: &[Block], shift: isize) -> (usize, usize) {
    let mut first = 0;
    while first < old.len() && first < new.len() && same_block(&old[first], &new[first], 0) {
        first += 1;
    }
    let room = old.len().min(new.len()) - first;
    let mut tail = 0;
    while tail < room
        && same_block(
            &old[old.len() - 1 - tail],
            &new[new.len() - 1 - tail],
            shift,
        )
    {
        tail += 1;
    }
    (first, tail)
}

fn shift_by(v: usize, by: isize) -> usize {
    usize::try_from(v as isize + by).unwrap_or(0)
}

/// Moves a page laid out before an edit to where it sits after it.
fn shift_page(page: &mut Page, meta: &mut PageMeta, blocks: isize, elements: isize) {
    for c in [&mut page.start, &mut page.next] {
        c.block = u32::try_from(i64::from(c.block) + blocks as i64).unwrap_or(0);
    }
    if elements != 0 {
        for l in &mut page.lines {
            if let Some((e, _, _)) = &mut l.source {
                *e = shift_by(*e, elements);
            }
        }
        for s in &mut meta.spans {
            s.0 = shift_by(s.0, elements);
        }
        for d in &mut meta.diagnostics {
            if let Some(e) = &mut d.element {
                *e = shift_by(*e, elements);
            }
        }
    }
}

impl Incremental {
    pub fn new(script: &Script, format: &ValidFormat, options: &Options) -> Incremental {
        let fmt = format.get().clone();
        let mut inc = Incremental {
            format: format.clone(),
            options: options.clone(),
            rules: Rules::default(),
            memo: Memo::default(),
            flow: Flow::default(),
            elements: 0,
            title_page: script.title_page.clone(),
            layout: Layout {
                title: Vec::new(),
                pages: Vec::new(),
                diagnostics: Vec::new(),
                spans: BTreeMap::new(),
                format: fmt,
            },
            metas: Vec::new(),
            stats: UpdateStats::default(),
        };
        inc.layout.title = inc.title_pages(script);
        inc.update(script);
        inc
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// What the last update did.
    pub fn stats(&self) -> &UpdateStats {
        &self.stats
    }

    /// The draft view of the script last laid out.
    pub fn draft(&self) -> crate::Draft {
        let starts: Vec<Cursor> = self.layout.pages.iter().map(|p| p.start).collect();
        Paginator {
            flow: &self.flow,
            format: self.format.get(),
            rules: self.rules,
        }
        .draft(&starts)
    }

    fn title_pages(&self, script: &Script) -> Vec<Page> {
        // Title pages depend only on the title page and the format; the
        // engine lays them out the same way a full layout does.
        let e = crate::Engine::new(
            &Script {
                title_page: script.title_page.clone(),
                elements: Vec::new(),
            },
            &self.format,
            &self.options,
        );
        e.run().title
    }

    /// Lays out the script after an edit. Any script may be given; the work
    /// done is proportional to how much of its flow and pages changed.
    pub fn update(&mut self, script: &Script) -> &Layout {
        if script.title_page != self.title_page {
            self.title_page = script.title_page.clone();
            self.layout.title = self.title_pages(script);
        }
        let format = self.format.get();
        let new_flow = flow::build(
            script,
            &FlowInput {
                format,
                overrides: &self.options.overrides,
                scene_numbers: self.options.scene_numbers,
                line_breaks: &self.options.line_breaks,
            },
            Some(&mut self.memo),
        );
        let shift = script.elements.len() as isize - self.elements as isize;
        self.elements = script.elements.len();
        let old = std::mem::replace(&mut self.flow, new_flow);
        let (first, tail) = diff(&old.blocks, &self.flow.blocks, shift);
        let new_len = self.flow.blocks.len();
        let block_shift = new_len as isize - old.blocks.len() as isize;
        let changed_end = new_len - tail;

        let p = Paginator {
            flow: &self.flow,
            format,
            rules: self.rules,
        };
        let pages = &mut self.layout.pages;
        let metas = &mut self.metas;
        let mut diagnostics = self.flow.diagnostics.clone();

        // Restart at the page before the first one that can see the change.
        let near = first.saturating_sub(LOOKAHEAD) as u32;
        let restart = pages
            .iter()
            .position(|pg| pg.next.block >= near)
            .unwrap_or(pages.len())
            .saturating_sub(1);
        let start = pages.get(restart).map_or(Cursor::START, |pg| pg.start);
        let old_starts: Vec<Cursor> = pages.iter().map(|pg| pg.start).collect();

        let mut fresh = Vec::new();
        let mut resume = None;
        let end = p.end();
        let mut c = p.canonical(start);
        while c < end {
            let body = p.page_from(c);
            if body.next <= c {
                diagnostics.push(no_progress());
                break;
            }
            c = body.next;
            fresh.push(body);
            // Past the changed blocks, a page that starts where an old one
            // did is that old page: everything it reads is unchanged.
            if c.block as usize >= changed_end && c < end {
                let old_c = Cursor {
                    block: u32::try_from(i64::from(c.block) - block_shift as i64).unwrap_or(0),
                    ..c
                };
                if let Ok(j) = old_starts.binary_search(&old_c)
                    && j > restart
                {
                    resume = Some(j);
                    break;
                }
            }
        }

        let mut kept: Vec<(Page, PageMeta)> = match resume {
            Some(j) => pages.drain(j..).zip(metas.drain(j..)).collect(),
            None => Vec::new(),
        };
        pages.truncate(restart);
        metas.truncate(restart);
        let laid_out = fresh.len();
        for body in fresh {
            let (page, meta) = number_page(&p, body, pages.len() as u32 + 1);
            pages.push(page);
            metas.push(meta);
        }
        let reused_after = kept.len();
        for (mut page, mut meta) in kept.drain(..) {
            shift_page(&mut page, &mut meta, block_shift, shift);
            let n = pages.len() as u32 + 1;
            if page.number != Some(n) {
                set_number_line(format, &mut page, n);
            }
            pages.push(page);
            metas.push(meta);
        }
        self.layout.diagnostics = diagnostics;
        self.layout.finish(&self.metas);
        self.stats = UpdateStats {
            first_changed_block: first,
            reused_before: restart,
            laid_out,
            reused_after,
        };
        &self.layout
    }
}
