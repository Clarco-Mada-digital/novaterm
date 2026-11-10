//! Arbre de splits binaire : chaque feuille est un panneau, chaque nœud une division.

use egui::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// Côte à côte (séparateur vertical).
    Horizontal,
    /// Empilé (séparateur horizontal).
    Vertical,
}

#[derive(Clone, Debug)]
pub enum SplitTree {
    Leaf(u64),
    Split {
        dir: Dir,
        ratio: f32,
        left: Box<SplitTree>,
        right: Box<SplitTree>,
    },
}

pub struct Divider {
    pub path: Vec<bool>,
    pub rect: Rect,
    pub dir: Dir,
    /// Dimension du parent le long de l'axe de division (pour convertir un drag en ratio).
    pub span: f32,
}

impl SplitTree {
    pub fn leaves(&self) -> Vec<u64> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves(&self, out: &mut Vec<u64>) {
        match self {
            Self::Leaf(id) => out.push(*id),
            Self::Split { left, right, .. } => {
                left.collect_leaves(out);
                right.collect_leaves(out);
            }
        }
    }

    pub fn contains(&self, id: u64) -> bool {
        match self {
            Self::Leaf(x) => *x == id,
            Self::Split { left, right, .. } => left.contains(id) || right.contains(id),
        }
    }

    /// Calcule les rectangles des feuilles et des séparateurs.
    pub fn layout(&self, rect: Rect, leaves: &mut Vec<(u64, Rect)>, dividers: &mut Vec<Divider>) {
        self.layout_inner(rect, Vec::new(), leaves, dividers);
    }

    fn layout_inner(
        &self,
        rect: Rect,
        path: Vec<bool>,
        leaves: &mut Vec<(u64, Rect)>,
        dividers: &mut Vec<Divider>,
    ) {
        match self {
            Self::Leaf(id) => leaves.push((*id, rect)),
            Self::Split { dir, ratio, left, right } => {
                let ratio = ratio.clamp(0.15, 0.85);
                let thickness = 6.0;
                let (a, b, div_rect, span) = match dir {
                    Dir::Horizontal => {
                        let split_x = rect.left() + rect.width() * ratio;
                        let a = Rect::from_min_max(rect.min, egui::pos2(split_x - thickness * 0.5, rect.bottom()));
                        let b = Rect::from_min_max(egui::pos2(split_x + thickness * 0.5, rect.top()), rect.max);
                        let d = Rect::from_min_max(
                            egui::pos2(split_x - thickness * 0.5, rect.top()),
                            egui::pos2(split_x + thickness * 0.5, rect.bottom()),
                        );
                        (a, b, d, rect.width())
                    }
                    Dir::Vertical => {
                        let split_y = rect.top() + rect.height() * ratio;
                        let a = Rect::from_min_max(rect.min, egui::pos2(rect.right(), split_y - thickness * 0.5));
                        let b = Rect::from_min_max(egui::pos2(rect.left(), split_y + thickness * 0.5), rect.max);
                        let d = Rect::from_min_max(
                            egui::pos2(rect.left(), split_y - thickness * 0.5),
                            egui::pos2(rect.right(), split_y + thickness * 0.5),
                        );
                        (a, b, d, rect.height())
                    }
                };
                dividers.push(Divider { path: path.clone(), rect: div_rect, dir: *dir, span });
                let mut lp = path.clone();
                lp.push(false);
                let mut rp = path;
                rp.push(true);
                left.layout_inner(a, lp, leaves, dividers);
                right.layout_inner(b, rp, leaves, dividers);
            }
        }
    }

    /// Divise la feuille `target` en deux, `new_id` devenant la seconde moitié.
    pub fn split(&mut self, target: u64, dir: Dir, new_id: u64) {
        match self {
            Self::Leaf(id) if *id == target => {
                let old = *id;
                *self = Self::Split {
                    dir,
                    ratio: 0.5,
                    left: Box::new(Self::Leaf(old)),
                    right: Box::new(Self::Leaf(new_id)),
                };
            }
            Self::Split { left, right, .. } => {
                left.split(target, dir, new_id);
                right.split(target, dir, new_id);
            }
            _ => {}
        }
    }

    /// Retire une feuille ; retourne l'arbre résultant (None si tout est retiré).
    pub fn removed(self, id: u64) -> Option<SplitTree> {
        match self {
            Self::Leaf(x) => {
                if x == id {
                    None
                } else {
                    Some(Self::Leaf(x))
                }
            }
            Self::Split { dir, ratio, left, right } => {
                let l = left.removed(id);
                let r = right.removed(id);
                match (l, r) {
                    (Some(l), Some(r)) => Some(Self::Split { dir, ratio, left: Box::new(l), right: Box::new(r) }),
                    (Some(l), None) => Some(l),
                    (None, Some(r)) => Some(r),
                    (None, None) => None,
                }
            }
        }
    }

    pub fn set_ratio(&mut self, path: &[bool], ratio: f32) {
        let mut node = self;
        for &go_right in path {
            match node {
                Self::Split { left, right, .. } => {
                    node = if go_right { right } else { left };
                }
                _ => return,
            }
        }
        if let Self::Split { ratio: r, .. } = node {
            *r = ratio.clamp(0.15, 0.85);
        }
    }

    pub fn ratio(&self, path: &[bool]) -> f32 {
        let mut node = self;
        for &go_right in path {
            match node {
                Self::Split { left, right, .. } => {
                    node = if go_right { right } else { left };
                }
                _ => return 0.5,
            }
        }
        match node {
            Self::Split { ratio, .. } => *ratio,
            _ => 0.5,
        }
    }

    /// Chemin (gauche=false / droite=true) jusqu'à la feuille `id`.
    pub fn path_to(&self, id: u64) -> Option<Vec<bool>> {
        match self {
            Self::Leaf(x) => (*x == id).then(Vec::new),
            Self::Split { left, right, .. } => {
                if let Some(mut p) = left.path_to(id) {
                    p.insert(0, false);
                    Some(p)
                } else if let Some(mut p) = right.path_to(id) {
                    p.insert(0, true);
                    Some(p)
                } else {
                    None
                }
            }
        }
    }

    fn dir_at(&self, path: &[bool]) -> Option<Dir> {
        let mut node = self;
        for &go_right in path {
            match node {
                Self::Split { left, right, .. } => node = if go_right { right } else { left },
                _ => return None,
            }
        }
        match node {
            Self::Split { dir, .. } => Some(*dir),
            _ => None,
        }
    }

    /// Déplace le séparateur le plus proche de `id` dans la direction `dir` :
    /// `delta` > 0 = vers la droite / le bas. Retourne false s'il n'y en a pas.
    pub fn nudge(&mut self, id: u64, dir: Dir, delta: f32) -> bool {
        let Some(path) = self.path_to(id) else { return false };
        for depth in (0..path.len()).rev() {
            let ancestor = &path[..depth];
            if self.dir_at(ancestor) == Some(dir) {
                let r = self.ratio(ancestor) + delta;
                self.set_ratio(ancestor, r);
                return true;
            }
        }
        false
    }

    /// Répartit l'espace équitablement (en tenant compte du nombre de feuilles).
    pub fn equalize(&mut self) {
        if let Self::Split { dir, ratio, left, right } = self {
            left.equalize();
            right.equalize();
            let l = left.count_along(*dir) as f32;
            let r = right.count_along(*dir) as f32;
            *ratio = l / (l + r);
        }
    }

    fn count_along(&self, d: Dir) -> usize {
        match self {
            Self::Leaf(_) => 1,
            Self::Split { dir, left, right, .. } => {
                if *dir == d {
                    left.count_along(d) + right.count_along(d)
                } else {
                    left.count_along(d).max(right.count_along(d))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nudge_and_equalize() {
        let mut t = SplitTree::Leaf(1);
        t.split(1, Dir::Horizontal, 2);
        t.split(2, Dir::Horizontal, 3);
        assert!(t.nudge(3, Dir::Horizontal, 0.1));
        assert!((t.ratio(&[true]) - 0.6).abs() < 1e-4);
        assert!(!t.nudge(3, Dir::Vertical, 0.1));
        t.equalize();
        assert!((t.ratio(&[]) - 1.0 / 3.0).abs() < 1e-4);
        assert!((t.ratio(&[true]) - 0.5).abs() < 1e-4);
    }
}
