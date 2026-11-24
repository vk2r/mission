import QtQuick
import qs.Commons
import qs.Ui

// The one header every page shares: a fixed-height row, a gap and the separator.
// Give it `heading` for the back button + heading, or fill `leading` with your own
// content; `trailing` takes the page's controls. The empty parts drag the floating card.
Column {
  id: header
  required property var host
  property string heading: ""
  default property alias leading: leftSlot.data
  property alias trailing: rightSlot.data

  readonly property real rowHeight: Math.max(Style.space(32), probe.implicitHeight)
  readonly property real gap: Style.space(12)

  width: parent ? parent.width : 0
  spacing: header.gap

  Item {
    width: parent.width
    height: header.rowHeight

    // Sizes the row like the home header: its tallest control is a button.
    Button {
      id: probe
      visible: false
      iconText: "󰒓"
      fontFamily: header.host.fontFamily
    }

    FloatDrag {
      anchors.fill: parent
      host: header.host
    }

    Item {
      id: leftSlot
      anchors.left: parent.left
      anchors.right: rightSlot.left
      anchors.rightMargin: Style.space(8)
      height: parent.height
    }

    Row {
      id: backRow
      visible: header.heading !== ""
      anchors.left: parent.left
      anchors.right: rightSlot.left
      anchors.rightMargin: Style.space(8)
      anchors.verticalCenter: parent.verticalCenter
      spacing: Style.space(12)

      Button {
        id: backButton
        anchors.verticalCenter: parent.verticalCenter
        iconText: "󰁍"
        bordered: false
        foreground: header.host.foreground
        fontFamily: header.host.fontFamily
        tooltipText: header.host.t("backTip")
        onClicked: header.host.goTo("home")
      }

      Text {
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, backRow.width - backButton.width - backRow.spacing)
        textFormat: Text.PlainText
        text: header.heading
        color: header.host.foreground
        font.family: header.host.fontFamily
        font.pixelSize: Style.font.title
        font.bold: true
        elide: Text.ElideRight
      }
    }

    Row {
      id: rightSlot
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
    }
  }

  PanelSeparator { width: parent.width; foreground: header.host.foreground }
}
