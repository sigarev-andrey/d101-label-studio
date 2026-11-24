//! Keep the displayed image intact while WinUI decodes the next frame.
//! At most one decode and the latest requested frame are retained.

use windows_reactor::*;

#[derive(Clone, PartialEq)]
struct ImageInput {
    image: EncodedImage,
    width: f64,
    height: f64,
    scale: f64,
}

struct BufferedImage(Preview<EncodedImage>);
impl Component for BufferedImage {
    type Input = ImageInput;
    type Message = (Decode, bool);
    fn create(input: &ImageInput, _: &ComponentContext<Self>) -> Self {
        let mut preview = Preview::default();
        preview.request(input.image.clone());
        Self(preview)
    }
    fn input_changed(&mut self, input: &ImageInput, _: &ComponentContext<Self>) {
        self.0.request(input.image.clone());
    }
    fn update(&mut self, (decode, success): Self::Message, _: &ComponentContext<Self>) {
        self.0.complete(decode, success);
    }
    fn view(&self, input: &ImageInput, ctx: &mut ViewContext<Self>) -> View {
        ScrollViewer::new()
            .width(input.width)
            .height(input.height)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .vertical_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .content(
                Grid::new()
                    .width(input.width)
                    .height(input.height)
                    .keyed_children(self.0.frames.iter().enumerate().map(|(slot, frame)| {
                        let (width, height) =
                            frame.as_ref().map(|f| png_size(&f.image)).unwrap_or((0, 0));
                        let mut image = Image::new()
                            .width(width as f64 * input.scale)
                            .height(height as f64 * input.scale)
                            .horizontal_alignment(HorizontalAlignment::Left)
                            .vertical_alignment(VerticalAlignment::Top)
                            .stretch(Stretch::Fill)
                            .opacity(if self.0.front == Some(slot) { 1.0 } else { 0.0 });
                        if let Some(frame) = frame {
                            image = image
                                .source_data(frame.image.clone())
                                .on_opened(ctx.message((frame.decode, true)))
                                .on_failed(ctx.message((frame.decode, false)));
                        }
                        KeyedView::new(slot.to_string(), image)
                    })),
            )
    }
}

/// Moving the containing Canvas child does not change this component's input.
pub fn image_view(image: &EncodedImage, width: f64, height: f64, scale: f64) -> View {
    View::component::<BufferedImage>(ImageInput {
        image: image.clone(),
        width,
        height,
        scale,
    })
}

fn png_size(image: &EncodedImage) -> (u32, u32) {
    let b = image.as_bytes(); // All sources are locally encoded PNGs with an IHDR header.
    (
        u32::from_be_bytes(b[16..20].try_into().unwrap()),
        u32::from_be_bytes(b[20..24].try_into().unwrap()),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decode {
    pub slot: usize,
    pub revision: u64,
}

pub struct Frame<T> {
    pub decode: Decode,
    pub image: T,
    ready: bool,
}

pub struct Preview<T> {
    pub frames: [Option<Frame<T>>; 2],
    pub front: Option<usize>,
    loading: Option<Decode>,
    pending: Option<T>,
    revision: u64,
}

impl<T> Default for Preview<T> {
    fn default() -> Self {
        Self {
            frames: [None, None],
            front: None,
            loading: None,
            pending: None,
            revision: 0,
        }
    }
}

impl<T: PartialEq> Preview<T> {
    pub fn request(&mut self, image: T) {
        if let Some(loading) = self.loading {
            // Do not restart a decode on every pointer event: it must be allowed to finish.
            self.pending =
                (self.frames[loading.slot].as_ref().unwrap().image != image).then_some(image);
            return;
        }
        if self
            .front
            .is_some_and(|slot| self.frames[slot].as_ref().is_some_and(|f| f.image == image))
        {
            return;
        }
        let slot = self.front.map_or(0, |front| 1 - front);
        if self.frames[slot]
            .as_ref()
            .is_some_and(|f| f.ready && f.image == image)
        {
            // An unchanged Image.Source will not raise another ImageOpened event.
            self.front = Some(slot);
            return;
        }
        self.revision += 1;
        let decode = Decode {
            slot,
            revision: self.revision,
        };
        self.frames[slot] = Some(Frame {
            decode,
            image,
            ready: false,
        });
        self.loading = Some(decode);
    }

    /// Ignore callbacks for retired requests. A failed decode never hides the current frame.
    pub fn complete(&mut self, decode: Decode, success: bool) -> bool {
        if self.loading != Some(decode) {
            return false;
        }
        self.loading = None;
        if success {
            self.frames[decode.slot].as_mut().unwrap().ready = true;
            self.front = Some(decode.slot);
        } else {
            self.frames[decode.slot] = None;
        }
        if let Some(latest) = self.pending.take() {
            self.request(latest);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible(preview: &Preview<u32>) -> Option<u32> {
        preview
            .front
            .map(|i| preview.frames[i].as_ref().unwrap().image)
    }
    fn ready(preview: &mut Preview<u32>, value: u32) {
        preview.request(value);
        let decode = preview.loading.unwrap();
        assert!(preview.complete(decode, true));
    }

    #[test]
    fn rapid_drag_keeps_visible_frame_and_finishes_at_latest_position() {
        let mut preview = Preview::default();
        ready(&mut preview, 0);
        preview.request(1);
        let first_decode = preview.loading.unwrap();
        for position in 2..100 {
            preview.request(position);
            assert_eq!(visible(&preview), Some(0));
            assert_eq!(preview.loading, Some(first_decode));
        }
        preview.complete(first_decode, true);
        assert_eq!(visible(&preview), Some(1));
        let last_decode = preview.loading.unwrap();
        assert_eq!(preview.frames[last_decode.slot].as_ref().unwrap().image, 99);
        preview.complete(last_decode, true);
        assert_eq!(visible(&preview), Some(99));
        assert!(preview.loading.is_none());
        assert!(preview.pending.is_none());
    }

    #[test]
    fn decode_failure_and_late_callbacks_cannot_erase_a_visible_frame() {
        let mut preview = Preview::default();
        ready(&mut preview, 0);
        preview.request(1);
        let failed = preview.loading.unwrap();
        preview.request(2);
        preview.complete(failed, false);
        assert_eq!(visible(&preview), Some(0));
        assert!(!preview.complete(failed, true));
        let next = preview.loading.unwrap();
        preview.complete(next, true);
        assert_eq!(visible(&preview), Some(2));
        assert!(!preview.complete(failed, false));
        assert_eq!(visible(&preview), Some(2));
    }

    #[test]
    fn reversing_drag_reuses_a_decoded_frame_without_waiting_for_another_event() {
        let mut preview = Preview::default();
        ready(&mut preview, 0);
        ready(&mut preview, 1);
        preview.request(0);
        assert_eq!(visible(&preview), Some(0));
        assert!(preview.loading.is_none());
        preview.request(2);
        let loading = preview.loading.unwrap();
        preview.request(3);
        preview.request(2); // Latest pointer position returns to the in-flight frame.
        preview.complete(loading, true);
        assert_eq!(visible(&preview), Some(2));
        assert!(preview.loading.is_none());
    }
}
