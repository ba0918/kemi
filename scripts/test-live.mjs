// `--live`（docs/spec/live.md、live-compare.md）のブラウザ自動化。実際の kemi バイナリを、
// scripts/live-dev-server.mjs の試験用の開発サーバを相手に起動し、画面を agent-browser で確かめる。
//
//   node scripts/test-live.mjs <kemi-bin>               全部の場面を決まった順に
//   node scripts/test-live.mjs <kemi-bin> <場面>...      並べた場面だけを並べた順に
//
// 場面は場面の関数の名前で選ぶ（例 `placesGlowFromTheListAndTheThread`）。`unreadableResourcesMakeNoPartialCommentImage:canvas`
// のように `:` の後ろを渡すと、その場面は渡したものだけを確かめる。
//
// 場面は領域ごとに scripts/live/ に分けてある: relay-and-view.mjs（中継とページの見方）、snapshots-and-mocks.mjs
// （スナップショット・モック・比べる相手）、changes-and-marks.mjs（変化の一覧と印）、page-comments.mjs（ページへの
// コメントと画像）。場面どうしで使う補助は common.mjs。確かめることは、場面ごとの関数のドキュメントコメントに書く。
import { close, makeRepository } from './live/common.mjs';
import {
  relayCarriesHmrAndHidesTheCookie,
  waitsForTheDevServer,
  fileReloadsWhenItsCssIsSaved,
  pageViewShowsFramedPagesWidthsAndNarrowScreens,
  modeTabsSwitchTheTopbar,
  outsideGitFilePages,
  handingFromTheCodeViewOutsideGit,
  handingFromTheExplanationView,
  otherReviewsLoadNoPageFiles,
  pageSurroundingsMatchTheReviewWithoutRepaintingThePage,
  narrowToolsFloatWithHand,
  narrowPageViewFitsOneRow,
  crossingTheNarrowWidthFollowsTheShownPage,
} from './live/relay-and-view.mjs';
import {
  snapshotsAreTakenShownAndChosen,
  snapshotsCarryTheirResources,
  mocksAreAssignedShownAndKeptApart,
  snapshotsSendNoTokenToExternalImages,
  overlayFollowsTheScrollAndTheOpacity,
  compareModesScaleAndReload,
  referenceNamesHeadingsAndNotices,
  recordingNowSwitchesTheReference,
  changesFromStartExplainHowToCompareFromNow,
  mockPanelListsTheFilesToChoose,
  mockMenuRemovesAndUndoes,
  sideBySidePagesShareTheirTop,
  snapshotsAndMocksComeBackAfterResuming,
} from './live/snapshots-and-mocks.mjs';
import {
  changesShowWithThePageAlone,
  changeRowsAreElementsThatLeadToThePage,
  removedRowsLeadToTheSnapshot,
  wideTargetsAreRevealedSideways,
  overlaidRemovedElementsAreRevealedWhereTheyAreDrawn,
  changeListFollowsThePage,
  marksFollowTheChanges,
  marksCanBeHidden,
  marksAreDrawnUnderAStrictStylePolicy,
  snapshotsAreTakenUnderTrustedTypes,
  removedMarksSurviveReparsing,
  scrollingMakesNoChangeAndMarksStay,
  cssomChangesAreFollowed,
  widthSwitchesWithoutResizingTheDocument,
  manyElementsAreRecordedAndCompared,
} from './live/changes-and-marks.mjs';
import {
  toolsAndTheHintStartTheFirstComment,
  closingTheComposeBoxRestoresTheFrame,
  foldedHandNoteLeavesTheComposeControlsClickable,
  commentImagesLookLikeThePage,
  unreadableResourcesMakeNoPartialCommentImage,
  commentImagesKeepTheLayoutOfTheViewport,
  pageCommentPlacesArePutAndSaved,
  penPlacesLeaveOutWhatContainsTheLine,
  penGlowAndImageStayAroundTheDrawnArea,
  placesOnTheBackgroundAreAreasOnly,
  savedCommentsAreQuietMarks,
  draftPlacesStayAtTheirWidth,
  latePlacesKeepTheirWidth,
  draftsStayWhileSaving,
  narrowReferenceSideSavesTheImage,
  switchingWhileSavingKeepsTheImageAroundThePlace,
  savingAndHandingKeepThePageLaidOut,
  movingWhileSavingMakesNoImageOfAnotherPage,
  placesMakeNoChange,
  savedPageCommentsAreListedShownAndSwitched,
  placesGlowFromTheListAndTheThread,
  handedPageCommentsReachWaitAndSubmit,
} from './live/page-comments.mjs';

/** 全部を走らせるときの場面の順。 */
const SCENES = [
  relayCarriesHmrAndHidesTheCookie,
  waitsForTheDevServer,
  fileReloadsWhenItsCssIsSaved,
  pageViewShowsFramedPagesWidthsAndNarrowScreens,
  modeTabsSwitchTheTopbar,
  outsideGitFilePages,
  handingFromTheCodeViewOutsideGit,
  handingFromTheExplanationView,
  otherReviewsLoadNoPageFiles,
  snapshotsAreTakenShownAndChosen,
  snapshotsCarryTheirResources,
  commentImagesLookLikeThePage,
  unreadableResourcesMakeNoPartialCommentImage,
  commentImagesKeepTheLayoutOfTheViewport,
  mocksAreAssignedShownAndKeptApart,
  mockPanelListsTheFilesToChoose,
  mockMenuRemovesAndUndoes,
  snapshotsSendNoTokenToExternalImages,
  overlayFollowsTheScrollAndTheOpacity,
  compareModesScaleAndReload,
  referenceNamesHeadingsAndNotices,
  recordingNowSwitchesTheReference,
  changesFromStartExplainHowToCompareFromNow,
  sideBySidePagesShareTheirTop,
  changesShowWithThePageAlone,
  toolsAndTheHintStartTheFirstComment,
  closingTheComposeBoxRestoresTheFrame,
  foldedHandNoteLeavesTheComposeControlsClickable,
  pageSurroundingsMatchTheReviewWithoutRepaintingThePage,
  narrowPageViewFitsOneRow,
  narrowToolsFloatWithHand,
  crossingTheNarrowWidthFollowsTheShownPage,
  changeListFollowsThePage,
  changeRowsAreElementsThatLeadToThePage,
  removedRowsLeadToTheSnapshot,
  wideTargetsAreRevealedSideways,
  overlaidRemovedElementsAreRevealedWhereTheyAreDrawn,
  marksFollowTheChanges,
  marksCanBeHidden,
  marksAreDrawnUnderAStrictStylePolicy,
  snapshotsAreTakenUnderTrustedTypes,
  removedMarksSurviveReparsing,
  scrollingMakesNoChangeAndMarksStay,
  cssomChangesAreFollowed,
  widthSwitchesWithoutResizingTheDocument,
  manyElementsAreRecordedAndCompared,
  pageCommentPlacesArePutAndSaved,
  penPlacesLeaveOutWhatContainsTheLine,
  penGlowAndImageStayAroundTheDrawnArea,
  placesOnTheBackgroundAreAreasOnly,
  savedCommentsAreQuietMarks,
  draftPlacesStayAtTheirWidth,
  latePlacesKeepTheirWidth,
  draftsStayWhileSaving,
  narrowReferenceSideSavesTheImage,
  switchingWhileSavingKeepsTheImageAroundThePlace,
  savingAndHandingKeepThePageLaidOut,
  movingWhileSavingMakesNoImageOfAnotherPage,
  placesMakeNoChange,
  savedPageCommentsAreListedShownAndSwitched,
  placesGlowFromTheListAndTheThread,
  handedPageCommentsReachWaitAndSubmit,
  snapshotsAndMocksComeBackAfterResuming,
];

/** `:` の後ろを受け取る場面。 */
const TAKES_ARGUMENT = new Set([unreadableResourcesMakeNoPartialCommentImage]);

/** 引数に並んだ場面（無ければ全部）。知らない名前と、`:` の後ろを受け取らない場面に渡したものは断る。 */
function chosenScenes(args) {
  if (args.length === 0) {
    return SCENES.map((scene) => ({ scene, argument: undefined }));
  }
  return args.map((arg) => {
    const [name, argument] = arg.split(/:(.*)/s);
    const scene = SCENES.find((each) => each.name === name);
    if (scene === undefined) {
      console.error(`unknown scene: ${name}\nscenes:\n${SCENES.map((each) => `  ${each.name}`).join('\n')}`);
      process.exit(2);
    }
    if (argument !== undefined && !TAKES_ARGUMENT.has(scene)) {
      console.error(`${name} takes no argument: ${arg}`);
      process.exit(2);
    }
    return { scene, argument };
  });
}

const chosen = chosenScenes(process.argv.slice(3));
try {
  for (const { scene, argument } of chosen) {
    await scene(await makeRepository(), argument);
  }
} finally {
  await close();
}
